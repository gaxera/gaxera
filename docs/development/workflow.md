# Gaxera Developer Workflow

This document outlines the standard workflows for bootstrapping, compiling, running, and testing the Gaxera microkernel.

---

## 1. Prerequisites

Before building Gaxera, ensure you have the following host tools installed in your development environment:

* **QEMU Emulator:** `qemu-system-x86_64` (with standard VGA and OVMF UEFI support)
* **ISO Packager:** `xorriso` (for El Torito hybrid ISO9660 packaging)
* **Core Utilities:** `curl`, `tar`, and `make` (to acquire and compile Limine stubs)

On Ubuntu/WSL, these can be installed via:

```bash
sudo apt update && sudo apt install -y qemu-system-x86 qemu-utils xorriso curl tar make
```

---

## 2. Developer Commands (cargo xtask)

Gaxera uses a host-side Rust build tool (`xtask`) to automate compilation and packaging. The root `.cargo/config.toml` aliases this to `cargo xtask`.

### A. Bootstrapping the Toolchain

Before your first build, or after cleaning your workspace, you must download and compile the Limine bootloader stubs:

```bash
cargo xtask bootstrap
```

This task:

1. Downloads the official Limine v12.4.2 binary distribution.
2. Verifies the SHA-256 checksum of the downloaded tarball.
3. Compiles the native `limine` host deployment tool.
4. Stages BIOS and UEFI boot stubs in `target/limine/`.

### B. Compiling the ISO

To compile the kernel and build a bootable hybrid ISO image:

```bash
cargo xtask build
```

This task:

1. Compiles the freestanding microkernel for the target architecture (`x86_64-unknown-none`).
2. Creates an ISO9660 staging structure under `target/iso_root/`.
3. Invokes `xorriso` to write a hybrid ISO (`target/gaxera.iso`) containing MBR/GPT partition records and UEFI El Torito boot sectors.
4. Deploys the Limine sector boot record into the ISO.

### C. Running Gaxera in QEMU

To compile, package, and launch Gaxera inside the emulator:

```bash
# Launch under UEFI (default if OVMF is installed) or BIOS:
cargo xtask run

# Force legacy BIOS mode:
cargo xtask run -- --firmware bios

# Run headless (outputs serial diagnostics directly to your shell):
cargo xtask run -- --headless
```

### D. Running Verification Tests

To run the complete automated integration test suite:

```bash
cargo xtask test
```

This command runs:

1. Compiler checks for the freestanding kernel.
2. Headless BIOS normal boot validation.
3. Headless UEFI normal boot validation.
4. Headless UEFI panic telemetry probe.
