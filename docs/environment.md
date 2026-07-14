# Gaxera Development Environment

This document records the exact toolchain and package versions used in Gaxera's primary development environment.

## Target Architecture
*   Target: `x86_64-unknown-none`
*   Firmware: UEFI (Limine Boot Protocol)

## Toolchain Configuration

### Rust Compiler
*   **Channel/Pin:** `nightly-2026-07-13`
*   **Rustc Version:** `rustc 1.99.0-nightly (77cf889bc 2026-07-12)`
*   **Components:** `rust-src`, `llvm-tools-preview`, `clippy`, `rustfmt`
*   **Target:** `x86_64-unknown-none`

### Host Environment (WSL2)
*   **Host OS:** Ubuntu 26.04 LTS (resolute)
*   **QEMU Emulator:** `qemu-system-x86_64` version `10.2.1`
*   **ISO Creator:** `xorriso` version `1.5.6`
*   **FAT Tooling:** `mtools` version `4.0.49`
*   **Debugger:** `gdb` version `17.1`

---

## Local Verification Checks

To verify that the development environment is active and consistent, run:

```bash
# Check compiler version
cargo --version

# Verify target compilation
cargo check --package kernel --target x86_64-unknown-none -Z build-std=core,compiler_builtins,alloc -Z build-std-features=compiler-builtins-mem
```
