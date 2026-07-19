use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy)]
enum Firmware {
    Bios,
    Uefi,
}

#[derive(Clone, Copy)]
enum ExceptionTest {
    Breakpoint,
    DivideError,
    InvalidOpcode,
    GeneralProtection,
    PageFault,
    DoubleFault,
}

#[derive(Clone, Copy)]
enum KernelProfile {
    Normal,
    PanicTest,
    BootTest,
    MemoryFoundation,
    HeapGuard,
    ApicTimer,
    UserTransition,
    UserPrivilege,
    UserInvalidFrame,
    SyscallRoundTrip,
    UserCopyFault,
    CooperativeYield,
    ContextPreservation,
    IpcTest,
    PreemptionTest,
    Exception(ExceptionTest),
    InitTest,
}

impl KernelProfile {
    fn features(self) -> Option<&'static str> {
        match self {
            Self::Normal => None,
            Self::PanicTest => Some("panic-test"),
            Self::BootTest => Some("test-boot"),
            Self::MemoryFoundation => Some("test-memory"),
            Self::HeapGuard => Some("test-heap-guard"),
            Self::ApicTimer => Some("test-apic-timer"),
            Self::UserTransition => Some("test-user-transition"),
            Self::UserPrivilege => Some("test-user-privilege"),
            Self::UserInvalidFrame => Some("test-user-invalid-frame"),
            Self::SyscallRoundTrip => Some("test-syscall-round-trip"),
            Self::UserCopyFault => Some("test-user-copy-fault"),
            Self::CooperativeYield => Some("test-cooperative-yield"),
            Self::ContextPreservation => Some("test-context-preservation"),
            Self::IpcTest => Some("test-ipc"),
            Self::PreemptionTest => Some("test-preemption"),
            Self::Exception(ExceptionTest::Breakpoint) => Some("test-breakpoint"),
            Self::Exception(ExceptionTest::DivideError) => Some("test-divide-error"),
            Self::Exception(ExceptionTest::InvalidOpcode) => Some("test-invalid-opcode"),
            Self::Exception(ExceptionTest::GeneralProtection) => Some("test-general-protection"),
            Self::Exception(ExceptionTest::PageFault) => Some("test-page-fault"),
            Self::Exception(ExceptionTest::DoubleFault) => Some("test-double-fault"),
            Self::InitTest => Some("qemu-test"),
        }
    }

    fn expected_markers(self) -> &'static [&'static str] {
        match self {
            Self::Normal | Self::BootTest => &["GAXERA: TEST_PATTERN_DRAWN"],
            Self::PanicTest => &[
                "GAXERA KERNEL PANIC at kernel/src/main.rs",
                "GAXERA: PANIC_DIAGNOSTICS_BEGIN",
                "GAXERA: PANIC_CPU_STATE",
                "GAXERA: PANIC_BACKTRACE_BEGIN",
                "GAXERA: PANIC_BACKTRACE_FRAME",
                "GAXERA: PANIC_BACKTRACE_END",
                "GAXERA: PANIC_DIAGNOSTICS_COMPLETE",
            ],
            Self::MemoryFoundation => &["GAXERA: MEMORY_FOUNDATION_OK"],
            Self::HeapGuard => &["GAXERA: HEAP_GUARD_PAGE_FAULT_CAUGHT"],
            Self::ApicTimer => &["GAXERA: APIC_TIMER_DELIVERY_OK"],
            Self::UserTransition => &["GAXERA: USER_TRANSITION_OK"],
            Self::UserPrivilege => &["GAXERA: USER_PRIVILEGE_DENIED_OK"],
            Self::UserInvalidFrame => &["GAXERA: USER_INVALID_FRAME_REJECTED"],
            Self::SyscallRoundTrip => &["GAXERA: SYSCALL_ROUND_TRIP_OK"],
            Self::UserCopyFault => &["GAXERA: USER_COPY_FAULT_RECOVERED_OK"],
            Self::CooperativeYield => &["GAXERA: COOPERATIVE_YIELD_OK"],
            Self::ContextPreservation => &["GAXERA: CONTEXT_PRESERVATION_OK"],
            Self::IpcTest => &["GAXERA: IPC_TEST_OK"],
            Self::PreemptionTest => &["GAXERA: PREEMPTION_OK"],
            Self::Exception(ExceptionTest::Breakpoint) => &["GAXERA: EXCEPTION_BREAKPOINT_RESUMED"],
            Self::Exception(ExceptionTest::DivideError) => {
                &["GAXERA: EXCEPTION_DIVIDE_ERROR_CAUGHT"]
            }
            Self::Exception(ExceptionTest::InvalidOpcode) => {
                &["GAXERA: EXCEPTION_INVALID_OPCODE_CAUGHT"]
            }
            Self::Exception(ExceptionTest::GeneralProtection) => {
                &["GAXERA: EXCEPTION_GENERAL_PROTECTION_CAUGHT"]
            }
            Self::Exception(ExceptionTest::PageFault) => &["GAXERA: EXCEPTION_PAGE_FAULT_CAUGHT"],
            Self::Exception(ExceptionTest::DoubleFault) => {
                &["GAXERA: EXCEPTION_DOUBLE_FAULT_IST_CAUGHT"]
            }
            Self::InitTest => &["GAXERA: FACTORY_INVOKED", "GAXERA: INIT_TEST_SUCCESS"],
        }
    }

    fn requires_guest_exit(self) -> bool {
        !matches!(self, Self::Normal)
    }

    fn log_name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::PanicTest => "panic",
            Self::BootTest => "boot",
            Self::MemoryFoundation => "memory",
            Self::HeapGuard => "heap-guard",
            Self::ApicTimer => "apic-timer",
            Self::UserTransition => "user-transition",
            Self::UserPrivilege => "user-privilege",
            Self::UserInvalidFrame => "user-invalid-frame",
            Self::SyscallRoundTrip => "syscall-round-trip",
            Self::UserCopyFault => "user-copy-fault",
            Self::CooperativeYield => "cooperative-yield",
            Self::ContextPreservation => "context-preservation",
            Self::IpcTest => "ipc-test",
            Self::PreemptionTest => "preemption",
            Self::Exception(ExceptionTest::Breakpoint) => "exception-breakpoint",
            Self::Exception(ExceptionTest::DivideError) => "divide-error",
            Self::Exception(ExceptionTest::InvalidOpcode) => "invalid-opcode",
            Self::Exception(ExceptionTest::GeneralProtection) => "general-protection",
            Self::Exception(ExceptionTest::PageFault) => "page-fault",
            Self::Exception(ExceptionTest::DoubleFault) => "double-fault",
            Self::InitTest => "init-scenario",
        }
    }
}

const TEST_TIMEOUT: Duration = Duration::from_secs(20);
const QEMU_DEBUG_EXIT_SUCCESS: i32 = 33;

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
            parse_firmware(&args)
                .and_then(|firmware| parse_profile(&args).map(|profile| (firmware, profile)))
                .and_then(|(firmware, profile)| handle_run(headless, firmware, profile))
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
    println!("  test         Run deterministic UEFI verification suite");
    println!("\nOptions:");
    println!("  --headless   Run QEMU without graphical display output");
    println!("  --firmware   Select uefi, or bios for an optional packaging diagnostic");
    println!("  --test       Run one deterministic proof: panic, memory, heap-guard, breakpoint,");
    println!(
        "               apic-timer, user-transition, user-privilege, user-invalid-frame, divide-error, invalid-opcode, general-protection, page-fault, or double-fault"
    );
}

fn parse_profile(args: &[String]) -> Result<KernelProfile, &'static str> {
    let Some(index) = args.iter().position(|arg| arg == "--test") else {
        return Ok(KernelProfile::Normal);
    };
    let value = args.get(index + 1).ok_or("--test requires a test name")?;
    match value.as_str() {
        "panic" => Ok(KernelProfile::PanicTest),
        "memory" => Ok(KernelProfile::MemoryFoundation),
        "heap-guard" => Ok(KernelProfile::HeapGuard),
        "apic-timer" => Ok(KernelProfile::ApicTimer),
        "user-transition" => Ok(KernelProfile::UserTransition),
        "user-privilege" => Ok(KernelProfile::UserPrivilege),
        "user-invalid-frame" => Ok(KernelProfile::UserInvalidFrame),
        "syscall-round-trip" => Ok(KernelProfile::SyscallRoundTrip),
        "user-copy-fault" => Ok(KernelProfile::UserCopyFault),
        "cooperative-yield" => Ok(KernelProfile::CooperativeYield),
        "context-preservation" => Ok(KernelProfile::ContextPreservation),
        "ipc-test" => Ok(KernelProfile::IpcTest),
        "preemption" => Ok(KernelProfile::PreemptionTest),
        "exception-breakpoint" => Ok(KernelProfile::Exception(ExceptionTest::Breakpoint)),
        "divide-error" => Ok(KernelProfile::Exception(ExceptionTest::DivideError)),
        "invalid-opcode" => Ok(KernelProfile::Exception(ExceptionTest::InvalidOpcode)),
        "general-protection" => Ok(KernelProfile::Exception(ExceptionTest::GeneralProtection)),
        "page-fault" => Ok(KernelProfile::Exception(ExceptionTest::PageFault)),
        "double-fault" => Ok(KernelProfile::Exception(ExceptionTest::DoubleFault)),
        "init-scenario" => Ok(KernelProfile::InitTest),
        _ => Err("unknown deterministic test name"),
    }
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

    // Step 2: Extract a complete source bundle. The CI cache can retain a
    // partially extracted target/ directory after an interrupted bootstrap.
    let source_is_complete =
        src_dir.join("Makefile").is_file() && src_dir.join("limine.c").is_file();
    if !source_is_complete {
        if src_dir.exists() {
            println!("Removing incomplete Limine source bundle...");
            if src_dir.is_dir() {
                fs::remove_dir_all(&src_dir)
                    .map_err(|_| "failed to remove incomplete Limine source bundle")?;
            } else {
                fs::remove_file(&src_dir)
                    .map_err(|_| "failed to remove invalid Limine source path")?;
            }
        }

        println!("Extracting Limine binary distribution...");
        let status = Command::new("tar")
            .args(["-xf", tarball_path.to_str().unwrap(), "-C", "target"])
            .status()
            .map_err(|_| "failed to execute tar")?;

        if !status.success() {
            return Err("failed to extract Limine binaries");
        }

        if !src_dir.join("Makefile").is_file() || !src_dir.join("limine.c").is_file() {
            return Err("Limine binary distribution is missing host-tool build inputs");
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
    handle_build_with_features(KernelProfile::Normal)
}

fn handle_build_with_features(profile: KernelProfile) -> Result<(), &'static str> {
    println!("=== Building Gaxera Kernel ISO ===");
    let limine_dir = Path::new("target/limine");
    if !limine_dir.join("limine").exists() {
        return Err("Limine stubs missing. Run 'cargo xtask bootstrap' first.");
    }

    // Step 1: Compile the kernel
    println!("Compiling kernel binary...");
    let mut build_args = vec![
        "build",
        "--locked",
        "--package",
        "kernel",
        "--target",
        "x86_64-unknown-none",
        "-Z",
        "build-std=core,compiler_builtins,alloc",
        "-Z",
        "build-std-features=compiler-builtins-mem",
    ];
    if let Some(features) = profile.features() {
        build_args.extend(["--features", features]);
    }
    let status = Command::new("cargo")
        .args(build_args)
        .status()
        .map_err(|_| "failed to execute cargo build")?;

    if !status.success() {
        return Err("kernel compilation failed");
    }

    // Step 1.5: Compile init binary (if present)
    let init_path = Path::new("crates/init");
    if init_path.exists() {
        println!("Compiling init binary...");
        let status = Command::new("cargo")
            .args([
                "build",
                "--locked",
                "--package",
                "init",
                "--target",
                "x86_64-unknown-none",
                "-Z",
                "build-std=core,compiler_builtins,alloc",
                "-Z",
                "build-std-features=compiler-builtins-mem",
            ])
            .status()
            .map_err(|_| "failed to execute cargo build for init")?;

        if !status.success() {
            return Err("init compilation failed");
        }
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

    // Copy built init ELF
    if init_path.exists() {
        fs::copy(
            "target/x86_64-unknown-none/debug/init",
            boot_dir.join("init.elf"),
        )
        .map_err(|_| "failed to copy init ELF to boot segment")?;
    }

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
    profile: KernelProfile,
) -> Result<(), &'static str> {
    handle_build_with_features(profile)?;

    println!("=== Launching QEMU Virtual Machine ===");

    // Check if OVMF UEFI firmware is present
    let ovmf_path = "/usr/share/ovmf/OVMF.fd";
    let firmware = match requested_firmware {
        Some(Firmware::Uefi) if !Path::new(ovmf_path).exists() => {
            return Err("OVMF UEFI firmware not found at /usr/share/ovmf/OVMF.fd");
        }
        Some(firmware) => firmware,
        None if Path::new(ovmf_path).exists() => Firmware::Uefi,
        None => return Err("OVMF UEFI firmware not found at /usr/share/ovmf/OVMF.fd"),
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

    if profile.requires_guest_exit() {
        // A triple fault must terminate the process rather than rebooting into
        // another firmware session; otherwise it can conceal a failed test.
        args.push("-no-reboot");
        args.push("-device");
        args.push("isa-debug-exit,iobase=0xf4,iosize=0x04");
    }

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

    let (line_sender, line_receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line_sender.send(line).is_err() {
                break;
            }
        }
    });

    // Setup local serial logging to a gitignored file
    let logs_dir = Path::new("logs");
    if !logs_dir.exists() {
        fs::create_dir_all(logs_dir).ok();
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock predates Unix epoch")?
        .as_secs();
    let mut log_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(logs_dir.join(format!("qemu-{}-{timestamp}.log", profile.log_name())))
        .ok();

    let expected_markers = profile.expected_markers();
    let mut markers_seen = vec![false; expected_markers.len()];
    let started = Instant::now();
    loop {
        let line = if profile.requires_guest_exit() {
            let Some(remaining) = TEST_TIMEOUT.checked_sub(started.elapsed()) else {
                child.kill().ok();
                child.wait().ok();
                return Err("QEMU test timed out before producing the expected result");
            };
            match line_receiver.recv_timeout(remaining) {
                Ok(line) => Some(line),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    child.kill().ok();
                    child.wait().ok();
                    return Err("QEMU test timed out before producing the expected result");
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => None,
            }
        } else {
            line_receiver.recv().ok()
        };

        let Some(line) = line else {
            break;
        };
        println!("{}", line);
        std::io::stdout().flush().ok();

        if let Some(ref mut file) = log_file {
            writeln!(file, "{}", line).ok();
            file.flush().ok();
        }

        for (index, marker) in expected_markers.iter().enumerate() {
            if line.contains(marker) {
                markers_seen[index] = true;
            }
        }
        let marker_seen = markers_seen.iter().all(|seen| *seen);

        if headless && marker_seen && !profile.requires_guest_exit() {
            println!("Success marker detected! Terminating emulation session.");
            child.kill().ok();
            child.wait().ok();
            return Ok(());
        }
    }

    let status = child.wait().map_err(|_| "failed to wait on QEMU process")?;
    let marker_seen = markers_seen.iter().all(|seen| *seen);
    if profile.requires_guest_exit() {
        if !marker_seen {
            println!("DEBUG: Markers seen: {:?}", markers_seen);
            println!("DEBUG: Expected markers: {:?}", expected_markers);
            return Err("QEMU exited without the expected kernel test marker");
        }
        if status.code() != Some(QEMU_DEBUG_EXIT_SUCCESS) {
            return Err("QEMU test did not report the expected isa-debug-exit success code");
        }
        println!("Guest-confirmed QEMU test passed.");
        return Ok(());
    }

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

fn lint_kernel_profile(profile: KernelProfile) -> Result<(), &'static str> {
    let mut lint_args = vec![
        "clippy",
        "--locked",
        "--package",
        "kernel",
        "--target",
        "x86_64-unknown-none",
        "-Z",
        "build-std=core,compiler_builtins,alloc",
        "-Z",
        "build-std-features=compiler-builtins-mem",
    ];
    if let Some(features) = profile.features() {
        lint_args.extend(["--features", features]);
    }
    lint_args.extend(["--", "-D", "warnings"]);

    println!("Linting kernel profile: {}", profile.log_name());
    let status = Command::new("cargo")
        .args(lint_args)
        .status()
        .map_err(|_| "failed to execute kernel clippy validation")?;
    if !status.success() {
        return Err("kernel clippy validation failed");
    }

    Ok(())
}

fn handle_test() -> Result<(), &'static str> {
    println!("=== Running Verification Tests ===");
    println!("Local target checking validation...");
    let status = Command::new("cargo")
        .args([
            "check",
            "--locked",
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

    println!("Running host-testable kernel, ABI, and core unit tests...");
    let status = Command::new("cargo")
        .args(["test", "--locked", "--package", "kernel", "--lib"])
        .status()
        .map_err(|_| "failed to execute host memory unit tests")?;
    if !status.success() {
        return Err("host memory unit tests failed");
    }

    let status = Command::new("cargo")
        .args([
            "test",
            "--locked",
            "--package",
            "gaxera-abi",
            "--package",
            "kernel-core",
        ])
        .status()
        .map_err(|_| "failed to execute host ABI and core unit tests")?;
    if !status.success() {
        return Err("host ABI and core unit tests failed");
    }

    println!("Strictly linting host ABI and core crates...");
    let status = Command::new("cargo")
        .args([
            "clippy",
            "--locked",
            "--package",
            "gaxera-abi",
            "--package",
            "kernel-core",
            "--",
            "-D",
            "warnings",
        ])
        .status()
        .map_err(|_| "failed to execute host ABI and core clippy validation")?;
    if !status.success() {
        return Err("host ABI and core clippy validation failed");
    }

    println!("Strictly linting every guest test profile...");
    for profile in [
        KernelProfile::Normal,
        KernelProfile::BootTest,
        KernelProfile::PanicTest,
        KernelProfile::MemoryFoundation,
        KernelProfile::HeapGuard,
        KernelProfile::ApicTimer,
        KernelProfile::UserTransition,
        KernelProfile::UserPrivilege,
        KernelProfile::UserInvalidFrame,
        KernelProfile::SyscallRoundTrip,
        KernelProfile::UserCopyFault,
        KernelProfile::CooperativeYield,
        KernelProfile::ContextPreservation,
        KernelProfile::IpcTest,
        KernelProfile::PreemptionTest,
        KernelProfile::Exception(ExceptionTest::Breakpoint),
        KernelProfile::Exception(ExceptionTest::DivideError),
        KernelProfile::Exception(ExceptionTest::InvalidOpcode),
        KernelProfile::Exception(ExceptionTest::GeneralProtection),
        KernelProfile::Exception(ExceptionTest::PageFault),
        KernelProfile::Exception(ExceptionTest::DoubleFault),
    ] {
        lint_kernel_profile(profile)?;
    }

    println!("Executing UEFI guest-confirmed integration checks...");
    handle_run(true, Some(Firmware::Uefi), KernelProfile::UserTransition)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::UserPrivilege)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::UserInvalidFrame)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::SyscallRoundTrip)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::UserCopyFault)?;

    println!("\n--- Cooperative Yield Test ---");
    handle_run(true, Some(Firmware::Uefi), KernelProfile::CooperativeYield)?;

    println!("\n--- Context Preservation Test ---");
    handle_run(
        true,
        Some(Firmware::Uefi),
        KernelProfile::ContextPreservation,
    )?;

    println!("\n--- IPC Test ---");
    handle_run(true, Some(Firmware::Uefi), KernelProfile::IpcTest)?;

    println!("\n--- Preemption Test ---");
    handle_run(true, Some(Firmware::Uefi), KernelProfile::PreemptionTest)?;

    println!("\n--- Hardware Exceptions ---");
    handle_run(true, Some(Firmware::Uefi), KernelProfile::BootTest)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::PanicTest)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::MemoryFoundation)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::HeapGuard)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::ApicTimer)?;
    handle_run(true, Some(Firmware::Uefi), KernelProfile::UserInvalidFrame)?;
    handle_run(
        true,
        Some(Firmware::Uefi),
        KernelProfile::Exception(ExceptionTest::Breakpoint),
    )?;
    handle_run(
        true,
        Some(Firmware::Uefi),
        KernelProfile::Exception(ExceptionTest::DivideError),
    )?;
    handle_run(
        true,
        Some(Firmware::Uefi),
        KernelProfile::Exception(ExceptionTest::InvalidOpcode),
    )?;
    handle_run(
        true,
        Some(Firmware::Uefi),
        KernelProfile::Exception(ExceptionTest::GeneralProtection),
    )?;
    handle_run(
        true,
        Some(Firmware::Uefi),
        KernelProfile::Exception(ExceptionTest::PageFault),
    )?;
    handle_run(
        true,
        Some(Firmware::Uefi),
        KernelProfile::Exception(ExceptionTest::DoubleFault),
    )?;

    println!("Restoring the normal kernel ISO after test-only builds...");
    handle_build()?;

    println!("All verification checks passed successfully!");
    Ok(())
}
