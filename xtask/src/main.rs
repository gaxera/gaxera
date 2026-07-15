use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Copy)]
enum Firmware {
    Bios,
    Uefi,
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_help();
        return;
    }

    let command = &args[1];
    let result = match command.as_str() {
        "bootstrap" => handle_bootstrap(),
        "build" => handle_build(),
        "run" => {
            let headless = args.contains(&"--headless".to_string());
            parse_firmware(&args).and_then(|firmware| handle_run(headless, firmware, false))
        }
        "clean" => handle_clean(),
        "test" => handle_test(),
        _ => {
            println!("Error: Unknown command '{}'", command);
            print_help();
            Err("unknown command")
        }
    };

    if let Err(e) = result {
        eprintln!("Task failed: {}", e);
        std::process::exit(1);
    }
}

fn print_help() {
    println!("Gaxera Build System");
    println!("Usage: cargo xtask <command> [options]");
    println!("\nCommands:");
    println!("  bootstrap    Download and compile Limine bootloader stubs");
    println!("  build        Compile kernel and package as bootable ISO");
    println!("  run          Build and launch QEMU emulator");
    println!("  clean        Remove build directories and toolchain caches");
    println!("  test         Run verification test suites");
    println!("\nOptions:");
    println!("  --headless   Run QEMU without graphical display output");
    println!("  --firmware   Select bios or uefi (defaults to UEFI when OVMF is installed)");
}

fn parse_firmware(args: &[String]) -> Result<Option<Firmware>, &'static str> {
    let Some(index) = args.iter().position(|arg| arg == "--firmware") else {
        return Ok(None);
    };
    let value = args
        .get(index + 1)
        .ok_or("--firmware requires bios or uefi")?;
    match value.as_str() {
        "bios" => Ok(Some(Firmware::Bios)),
        "uefi" => Ok(Some(Firmware::Uefi)),
        _ => Err("--firmware must be bios or uefi"),
    }
}

fn handle_bootstrap() -> Result<(), &'static str> {
    println!("=== Bootstrapping Limine Toolchain ===");
    let target_dir = Path::new("target");
    let limine_dir = target_dir.join("limine");
    let tarball_path = target_dir.join("limine-binary.tar.gz");
    let src_dir = target_dir.join("limine-binary");

    if !target_dir.exists() {
        fs::create_dir_all(target_dir).map_err(|_| "failed to create target/")?;
    }

    // Step 1: Download Limine binary distribution
    if !tarball_path.exists() {
        println!("Downloading Limine v12.4.2 binary distribution...");
        let status = Command::new("curl")
            .args([
                "-L",
                "-o",
                tarball_path.to_str().unwrap(),
                "https://github.com/Limine-Bootloader/Limine/releases/download/v12.4.2/limine-binary.tar.gz",
            ])
            .status()
            .map_err(|_| "failed to execute curl")?;

        if !status.success() {
            return Err("failed to download Limine binary distribution");
        }
    }

    // Verify SHA-256 checksum of the downloaded tarball before extraction
    let expected_hash = "0be070a0e41be9b13518ba578da7f577d1e7af4fd56c7254e2c39b58a5502e4d";
    println!("Verifying SHA-256 checksum for limine-binary.tar.gz...");
    let output = Command::new("sha256sum")
        .arg(tarball_path.to_str().unwrap())
        .output()
        .map_err(|_| "failed to execute sha256sum verification tool")?;
    if !output.status.success() {
        return Err("failed to calculate checksum of Limine binary tarball");
    }
    let output_str = String::from_utf8_lossy(&output.stdout);
    let calculated_hash = output_str
        .split_whitespace()
        .next()
        .ok_or("failed to parse sha256sum output")?;
    if calculated_hash != expected_hash {
        return Err("Limine tarball SHA-256 checksum validation failed! Supply chain mismatch.");
    }
    println!("Checksum verified successfully.");

    // Step 2: Extract tarball
    if !src_dir.exists() {
        println!("Extracting Limine binary distribution...");
        let status = Command::new("tar")
            .args(["-xf", tarball_path.to_str().unwrap(), "-C", "target"])
            .status()
            .map_err(|_| "failed to execute tar")?;

        if !status.success() {
            return Err("failed to extract Limine binaries");
        }
    }

    // Step 3: Compile Limine host executable (from limine.c in the binary bundle)
    println!("Compiling Limine host tools...");
    let status = Command::new("make")
        .current_dir(&src_dir)
        .status()
        .map_err(|_| "failed to execute make")?;

    if !status.success() {
        return Err("failed to compile Limine host tools");
    }

    // Step 4: Staging required binaries
    if !limine_dir.exists() {
        fs::create_dir_all(&limine_dir).map_err(|_| "failed to create target/limine/")?;
    }

    // Copy compiled host tool
    println!("Staging tool: limine");
    fs::copy(src_dir.join("limine"), limine_dir.join("limine"))
        .map_err(|_| "failed to stage limine host executable")?;

    // Copy boot stubs
    let stubs = [
        "limine-bios.sys",
        "limine-bios-cd.bin",
        "limine-uefi-cd.bin",
        "BOOTX64.EFI",
    ];

    for file in &stubs {
        let src = src_dir.join(file);
        let dest = limine_dir.join(file);
        println!("Staging boot stub: {}", file);
        fs::copy(&src, &dest).map_err(|_| "failed to stage bootloader stub")?;
    }

    println!("Limine toolchain bootstrap complete!");
    Ok(())
}

fn handle_build() -> Result<(), &'static str> {
    handle_build_with_features(None)
}

fn handle_build_with_features(features: Option<&str>) -> Result<(), &'static str> {
    println!("=== Building Gaxera Kernel ISO ===");
    let limine_dir = Path::new("target/limine");
    if !limine_dir.join("limine").exists() {
        return Err("Limine stubs missing. Run 'cargo xtask bootstrap' first.");
    }

    // Step 1: Compile the kernel
    println!("Compiling kernel binary...");
    let mut build_args = vec![
        "build",
        "--package",
        "kernel",
        "--target",
        "x86_64-unknown-none",
        "-Z",
        "build-std=core,compiler_builtins,alloc",
        "-Z",
        "build-std-features=compiler-builtins-mem",
    ];
    if let Some(features) = features {
        build_args.extend(["--features", features]);
    }
    let status = Command::new("cargo")
        .args(build_args)
        .status()
        .map_err(|_| "failed to execute cargo build")?;

    if !status.success() {
        return Err("kernel compilation failed");
    }

    // Step 2: Assemble ISO root directory
    println!("Assembling ISO directory structure...");
    let iso_root = Path::new("target/iso_root");
    let boot_dir = iso_root.join("boot");

    if iso_root.exists() {
        fs::remove_dir_all(iso_root).map_err(|_| "failed to purge old target/iso_root/")?;
    }
    fs::create_dir_all(&boot_dir).map_err(|_| "failed to create target/iso_root/boot/")?;

    // Copy built kernel ELF
    fs::copy(
        "target/x86_64-unknown-none/debug/kernel",
        boot_dir.join("gaxera.elf"),
    )
    .map_err(|_| "failed to copy kernel ELF to boot segment")?;

    // Copy limine configuration
    fs::copy("kernel/limine.conf", boot_dir.join("limine.conf"))
        .map_err(|_| "failed to copy limine.conf configuration")?;

    // Copy fallback limine configuration to ISO root
    fs::copy("kernel/limine.conf", iso_root.join("limine.conf"))
        .map_err(|_| "failed to copy limine.conf root configuration")?;

    // Copy bootloader files
    let boot_files = [
        "limine-bios.sys",
        "limine-bios-cd.bin",
        "limine-uefi-cd.bin",
    ];
    for file in &boot_files {
        fs::copy(limine_dir.join(file), boot_dir.join(file))
            .map_err(|_| "failed to copy Limine bootloader stub")?;
    }

    // Copy fallback stage-2 bootloader stub to ISO root
    fs::copy(
        limine_dir.join("limine-bios.sys"),
        iso_root.join("limine-bios.sys"),
    )
    .map_err(|_| "failed to copy limine-bios.sys root stub")?;

    // Copy UEFI boot stub to its standard path inside the ISO filesystem
    let efi_boot_dir = iso_root.join("EFI/BOOT");
    fs::create_dir_all(&efi_boot_dir).map_err(|_| "failed to create EFI/BOOT staging directory")?;
    fs::copy(
        limine_dir.join("BOOTX64.EFI"),
        efi_boot_dir.join("BOOTX64.EFI"),
    )
    .map_err(|_| "failed to stage BOOTX64.EFI to EFI/BOOT")?;
    fs::copy("kernel/limine.conf", efi_boot_dir.join("limine.conf"))
        .map_err(|_| "failed to stage UEFI Limine configuration")?;

    // Step 3: Run xorriso to create the ISO
    println!("Packaging bootable ISO image via xorriso...");
    let status = Command::new("xorriso")
        .args([
            "-as",
            "mkisofs",
            "-R",
            "-J",
            "-b",
            "boot/limine-bios-cd.bin",
            "-no-emul-boot",
            "-boot-load-size",
            "4",
            "-boot-info-table",
            "--efi-boot",
            "boot/limine-uefi-cd.bin",
            "-efi-boot-part",
            "--efi-boot-image",
            "--protective-msdos-label",
            "target/iso_root",
            "-o",
            "target/gaxera.iso",
        ])
        .status()
        .map_err(|_| "failed to execute xorriso. Is xorriso package installed?")?;

    if !status.success() {
        return Err("xorriso packaging execution failed");
    }

    // Step 4: Run limine deploy tool to establish boot records for BIOS compatibility
    println!("Installing bootloader record sector...");
    let status = Command::new(limine_dir.join("limine"))
        .args(["bios-install", "target/gaxera.iso"])
        .status()
        .map_err(|_| "failed to run limine bios deployment")?;

    if !status.success() {
        return Err("limine bios-install execution failed");
    }

    println!("Gaxera packaging successfully compiled to target/gaxera.iso!");
    Ok(())
}

fn handle_run(
    headless: bool,
    requested_firmware: Option<Firmware>,
    panic_test: bool,
) -> Result<(), &'static str> {
    handle_build_with_features(panic_test.then_some("panic-test"))?;

    println!("=== Launching QEMU Virtual Machine ===");

    // Check if OVMF UEFI firmware is present
    let ovmf_path = "/usr/share/ovmf/OVMF.fd";
    let firmware = match requested_firmware {
        Some(Firmware::Uefi) if !Path::new(ovmf_path).exists() => {
            return Err("OVMF UEFI firmware not found at /usr/share/ovmf/OVMF.fd");
        }
        Some(firmware) => firmware,
        None if Path::new(ovmf_path).exists() => Firmware::Uefi,
        None => Firmware::Bios,
    };

    let mut args = vec![];
    if matches!(firmware, Firmware::Uefi) {
        println!("OVMF UEFI firmware detected. Booting in UEFI mode.");
        args.push("-bios");
        args.push(ovmf_path);
        // A hybrid ISO must be attached as optical media. Attaching it with
        // -hda presents a raw hard disk and bypasses its UEFI El Torito entry.
        args.push("-cdrom");
        args.push("target/gaxera.iso");
    } else {
        println!("Booting in legacy BIOS mode.");
        args.push("-cdrom");
        args.push("target/gaxera.iso");
    }

    args.push("-serial");
    args.push("stdio");
    args.push("-vga");
    args.push("std");
    // Boot from CD-ROM (order=d) since we attach the hybrid ISO via -cdrom
    args.push("-boot");
    args.push("order=d,menu=off");
    args.push("-net");
    args.push("none");
    args.push("-m");
    args.push("512M");
    args.push("-cpu");
    args.push("max");

    if headless {
        println!("Headless mode selected.");
        args.push("-display");
        args.push("none");
    }

    // Spawn QEMU process with piped stdout and stdin
    let mut child = Command::new("qemu-system-x86_64")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|_| "failed to spawn QEMU process. Is QEMU installed?")?;

    // Send a newline to console input periodically to satisfy the UEFI "Press any key to boot" prompt
    if let Some(mut stdin) = child.stdin.take() {
        std::thread::spawn(move || {
            use std::io::Write;
            for _ in 0..10 {
                stdin.write_all(b"\n").ok();
                stdin.flush().ok();
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        });
    }

    let stdout = child
        .stdout
        .take()
        .ok_or("failed to open QEMU stdout stream")?;
    use std::io::{BufRead, BufReader, Write};
    let reader = BufReader::new(stdout);

    // Setup local serial logging to a gitignored file
    let logs_dir = Path::new("logs");
    if !logs_dir.exists() {
        fs::create_dir_all(logs_dir).ok();
    }
    let mut log_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(logs_dir.join("qemu_run.log"))
        .ok();

    // Read QEMU output stream line-by-line and write it to host terminal directly
    for line in reader.lines().map_while(Result::ok) {
        println!("{}", line);
        std::io::stdout().flush().ok();

        if let Some(ref mut file) = log_file {
            writeln!(file, "{}", line).ok();
            file.flush().ok();
        }

        // If running in headless/test mode, we can automatically exit QEMU once the entry is checked.
        let success_marker = if panic_test {
            "GAXERA KERNEL PANIC at kernel/src/main.rs"
        } else {
            "GAXERA: TEST_PATTERN_DRAWN"
        };
        if headless && line.contains(success_marker) {
            println!("Success marker detected! Terminating emulation session.");
            child.kill().ok();
            child.wait().ok();
            return Ok(());
        }
    }

    // Wait for the emulator process to terminate
    let status = child.wait().map_err(|_| "failed to wait on QEMU process")?;
    if !status.success() {
        return Err("QEMU session exited with error code");
    }

    Ok(())
}

fn handle_clean() -> Result<(), &'static str> {
    println!("=== Cleaning Workspace ===");
    let status = Command::new("cargo")
        .args(["clean"])
        .status()
        .map_err(|_| "failed to execute cargo clean")?;

    if !status.success() {
        return Err("cargo clean failed");
    }

    let limine_dir = Path::new("target/limine");
    if limine_dir.exists() {
        fs::remove_dir_all(limine_dir).map_err(|_| "failed to remove target/limine/")?;
    }

    println!("Workspace clean completed.");
    Ok(())
}

fn handle_test() -> Result<(), &'static str> {
    println!("=== Running Verification Tests ===");
    println!("Local target checking validation...");
    let status = Command::new("cargo")
        .args([
            "check",
            "--package",
            "kernel",
            "--target",
            "x86_64-unknown-none",
            "-Z",
            "build-std=core,compiler_builtins,alloc",
            "-Z",
            "build-std-features=compiler-builtins-mem",
        ])
        .status()
        .map_err(|_| "failed to validate compilation")?;

    if !status.success() {
        return Err("compilation checks failed");
    }

    println!("Executing entry marker integration check...");
    handle_run(true, Some(Firmware::Bios), false)?;
    handle_run(true, Some(Firmware::Uefi), false)?;
    handle_run(true, Some(Firmware::Uefi), true)?;

    println!("All verification checks passed successfully!");
    Ok(())
}
