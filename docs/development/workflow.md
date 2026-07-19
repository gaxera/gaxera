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
sudo apt update && sudo apt install -y qemu-system-x86 qemu-utils ovmf xorriso curl tar make
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

All Cargo invocations use the committed `Cargo.lock`; a dependency resolution
that would modify it fails instead of silently changing the build input.

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
# Launch under UEFI (the required development and verification target):
cargo xtask run

# Optional legacy-BIOS diagnostic path; not part of the supported architecture:
cargo xtask run -- --firmware bios

# Run headless (outputs serial diagnostics directly to your shell):
cargo xtask run -- --headless

# Run one guest-confirmed deterministic proof:
cargo xtask run -- --headless --test panic
cargo xtask run -- --headless --test boot
cargo xtask run -- --headless --test double-fault
cargo xtask run -- --headless --test memory
cargo xtask run -- --headless --test heap-guard
cargo xtask run -- --headless --test apic-timer
cargo xtask run -- --headless --test user-transition
cargo xtask run -- --headless --test user-privilege
cargo xtask run -- --headless --test user-invalid-frame
cargo xtask run -- --headless --test syscall-round-trip
cargo xtask run -- --headless --test user-copy-fault
cargo xtask run -- --headless --test cooperative-yield
cargo xtask run -- --headless --test context-preservation
cargo xtask run -- --headless --test ipc-test
cargo xtask run -- --headless --test preemption
cargo xtask run -- --headless --test init-scenario
```

### D. Running Verification Tests

To run the complete automated integration test suite:

```bash
cargo xtask test
```

This command runs:

1. Locked compiler checks, host-testable kernel, ABI, and capability-core unit
   tests, plus strict Clippy validation for those crates, the normal kernel,
   and every feature-gated guest-test profile.
2. Headless UEFI normal boot validation with a guest-confirmed QEMU exit after
   Gaxera captures its immutable boot context, switches to its own CR3,
   initializes its physical allocator, and initializes the guarded kernel heap.
3. A headless UEFI panic telemetry probe that requires the source location,
   CPU-state snapshot, a bounded frame-pointer backtrace, diagnostic
   completion, and a guest-confirmed QEMU exit.
4. A Phase 4 memory proof that requires successful `Box` and `Vec` allocation
   plus virtual-to-physical heap translation after the CR3 transition.
5. A Phase 4 heap-guard proof that deliberately faults on the unmapped lower
   guard page and confirms the exact address through CR2.
6. A Phase 5 ACPI/MADT and Local APIC proof that confirms the temporary
   firmware window is released and receives exactly three periodic timer
   deliveries with EOI handling.
7. A user-transition proof (M2A) that enters ring 3 through an `iretq`
   trampoline and returns through the `int 0x81` test gate, verifying the
   kernel transition stack bounds.
8. A user-privilege proof that executes `cli` (a privileged instruction) from
   CPL 3 and verifies the processor raises `#GP` as expected.
9. A user-invalid-frame proof that validates a malformed `UserTransitionFrame`
   (kernel code selector) is rejected before any transition occurs.
10. A syscall-round-trip proof that executes `syscall` from ring 3 with
    `rax = 0` (NoOp), returns through `sysretq`, and terminates through the
    test return gate.
11. A user-copy-fault proof that invokes `copy_from_user` with a null pointer,
    confirms the page-fault recovery path routes through the `CpuLocal`
    recovery record, and returns `Err(Fault)` without crashing.
12. A cooperative-yield proof that spawns two user-mode threads, executes
    `Syscall::Yield` from thread 0, context-switches to thread 1, and
    confirms successful return through the test gate.
13. A context-preservation proof that loads sentinel values into all six
    callee-saved registers (`rbx`, `r12`–`r15`, `rbp`) before a cooperative
    yield, context-switches to a second thread and back, then mathematically
    verifies every sentinel survived the round-trip.
14. An IPC proof that instantiates the core capability system, verifies synchronous endpoint rendezvous, executes an exact atomic capability-transfer rollback, and checks notification signaling.
15. A preemption proof that spins in user mode until the calibrated APIC timer interrupts it, causing a preemptive context switch into a second user thread that signals success.
16. UEFI breakpoint, divide error, invalid opcode, general protection fault,
    page fault, and double-fault probes.
17. A normal kernel rebuild after the test-only images so `target/gaxera.iso`
    never remains an injected-fault image.

The double-fault probe omits only its test-image page-fault gate, causes a real
page fault, and relies on processor escalation during exception delivery. The
handler reports success only after it confirms that RSP lies inside the static
IST stack; a stack mismatch exits QEMU with a failure status.

Legacy BIOS can be invoked manually as a packaging diagnostic, but it is not a required CI or release target. GitHub Actions invokes this same `cargo xtask test` command, so the entire normal, panic, memory, APIC, user-transition, syscall, scheduling, and exception matrix is part of CI rather than local-only checks.

The only production Limine boundary is `kernel/src/arch/x86_64/boot.rs`.
After it copies and publishes `&'static BootContext`, later setup consumes only
Gaxera-owned metadata; no allocator, framebuffer, paging, or entry code reads
a Limine response pointer. ACPI parsing consumes copied physical metadata and
uses only the fixed temporary firmware mapping; it retains no firmware-table
pointer. The closeout tag also triggers this CI workflow, so tagged source
revisions receive the same verification matrix.
