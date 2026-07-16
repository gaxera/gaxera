# Gaxera Development Environment

This document records the exact toolchain and package versions used in Gaxera's primary development environment.

## Target Architecture

* Target: `x86_64-unknown-none`
* Firmware: UEFI (Limine Boot Protocol)

## Toolchain Configuration

### Rust Compiler

* Channel/Pin: `nightly-2026-07-13`
* Rustc Version: `rustc 1.99.0-nightly (77cf889bc 2026-07-12)`
* Components: `rust-src`, `llvm-tools-preview`, `clippy`, `rustfmt`
* Target: `x86_64-unknown-none`

### Host Environment (WSL2)

* Host OS: Ubuntu 26.04 LTS (resolute)
* QEMU Emulator: `qemu-system-x86_64` version `10.2.1`
* ISO Creator: `xorriso` version `1.5.6`
* FAT Tooling: `mtools` version `4.0.49`
* Debugger: `gdb` version `17.1`

---

## Local Verification Checks

To verify that the development environment is active and consistent, run the unified verification checks:

```bash
cargo xtask test
```

---

## Support Matrix

Gaxera's Phase 3 exception and completed Phase 4 memory contract is verified
specifically under:

* QEMU emulated standard VGA adapter (`-vga std`).
* OVMF UEFI firmware, which is the required development, CI, and release target.
* The hybrid ISO's SeaBIOS path only as an optional packaging diagnostic.

The early framebuffer driver assumes 32-bit linear RGB format with byte-aligned 8-bit masks. It does not support physical hardware graphics cards, palette-indexed layouts, or non-byte-aligned Shift patterns.

The COM1 serial output writes directly to register ports without transmit status polling (`LSR`), which is designed for emulator console routing and can drop characters or overflow buffers on slower physical UART interfaces.

The exception matrix deliberately exercises breakpoint, division error, invalid
opcode, general protection fault, page fault, and a processor-escalated double
fault. It is QEMU/OVMF evidence, not a claim of physical-hardware validation.

The Phase 4 matrix additionally proves a Gaxera-owned CR3 transition, a
RAM-only higher-half direct map, a segmented physical-frame allocator, a fixed
2 MiB kernel heap, and the lower heap guard page. It verifies allocation with
`Box` and `Vec`, validates the first heap-page translation, and requires the
guard-page fault to report its exact address in CR2. These checks do not yet
validate SMP behavior, physical hardware, arbitrary MMIO mappings, huge pages,
or user address spaces.
