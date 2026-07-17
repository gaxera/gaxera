# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [v0.1.0] - 2026-07-17

### Added

- Bounded, allocation-free serial panic telemetry with source location, CPU
  state, and a validated frame-pointer backtrace over Gaxera-owned stacks.
- Exact bare-metal frame-pointer policy and a focused
  `cargo xtask run -- --headless --test panic` workflow.
- ADR 0006, defining panic-diagnostic ownership, unsafe invariants, and the
  deliberately limited raw-address backtrace contract.

### Changed

- The deterministic QEMU runner now verifies every required marker for a
  profile across the serial stream. The panic profile requires the location,
  CPU snapshot, frame output, bounded-walk completion, and guest exit.

## [Phase 5 Complete] - 2026-07-17

### Added

- Minimal Gaxera-owned ACPI revision 2+ RSDP, XSDT, and MADT parser with
  Local APIC address-override handling and copied-table ownership.
- Page-at-a-time ACPI-reclaimable temporary mappings and a typed uncached
  Local APIC mapping outside the RAM-only HHDM.
- BSP xAPIC initialization, legacy PIC masking, spurious-vector handling, and
  deterministic periodic timer delivery at vector `0xe0`.
- The `apic-timer` QEMU profile, expanded locked verification matrix, ADR 0005,
  and Checkpoint 5 exact-commit evidence.

### Changed

- The kernel now enters an interrupt-enabled idle path after APIC initialization
  in the normal production profile; timer calibration, scheduling, SMP, and
  general ACPI services remain explicitly deferred.

## [Phase 4 Complete] - 2026-07-16

### Added

- Immutable Gaxera-owned `BootContext` capture, a classified deterministic
  memory-map diagnostic, and a strict architecture-only Limine boundary.
- Bootstrap reservations, a bootstrap range allocator, and a segmented bitmap
  physical-frame allocator over `MEMMAP_USABLE` memory only.
- Gaxera-owned four-level tables, a RAM-only HHDM, W^X/NX page policy, guarded
  bootstrap/IST stacks, a 2 MiB guarded heap, and exact-pinned
  `linked_list_allocator = "=0.10.6"`.
- UEFI `memory` and `heap-guard` deterministic QEMU profiles, added to the
  complete `cargo xtask test` matrix and CI.
- ADRs 0003 and 0004 plus `PHASE_4_ENGINEERING_HANDOFF.md`.

### Changed

- The kernel moves to a 64 KiB static bootstrap stack before Rust entry;
  previous 32 KiB storage was insufficient for debug-build allocator setup.
- Limine request metadata is R+NX after CR3, arbitrary mapping is not exposed,
  and RSDP capture handles Limine's base-revision-specific address semantics.

## [Phase 3 Complete] - 2026-07-16

### Added

- Gaxera-owned x86-64 GDT, TSS, and a static 32 KiB double-fault IST stack.
- An IDT covering breakpoint, division error, invalid opcode, general protection fault, page fault, and double fault.
- Exact-pinned `x86_64 = "=0.15.5"` primitives for descriptor-table and interrupt ABI mechanics.
- Feature-gated UEFI exception probes and guest-confirmed QEMU exits through `isa-debug-exit`.
- A bounded UEFI QEMU test harness with `-no-reboot`, timestamped local logs, locked Cargo resolution, and normal-ISO restoration after test-only builds.
- ADR 0002 documenting descriptor-table ownership, terminal exception policy, and the controlled processor-delivered double-fault proof.

### Changed

- UEFI is the required Phase 3 validation target; BIOS remains an optional packaging diagnostic.
- `cargo xtask test` now validates normal boot, panic telemetry, and all Phase 3 exception probes with guest-confirmed outcomes; the double-fault success marker is emitted only after RSP is verified within the configured IST stack.

## [Phase 1 Complete] - 2026-07-14

### Added

- Standardized toolchain configuration pinning `nightly-2026-07-13` with `x86_64-unknown-none` target.
- Root Cargo workspace grouping `kernel` and `xtask` crates.
- Custom target compilation alias for `cargo xtask` build runner.
- Continuous Integration (CI) configuration at `.github/workflows/ci.yml`.
- System environmental verification documentation (`docs/environment.md`).
- Minimum `#![no_std]` and `#![no_main]` kernel entry point skeleton.

## [Foundation] - 2026-07-12

### Added

- Initial project repository foundation
- Governance documents: Constitution, Build Log, and Code of Conduct
- Technical documentation: Canonical Specification, Kernel Requirements, and Execution Roadmap
- Architecture Decision Record (ADR) framework and templates
- Design history preservation: 7 exploration sessions
- Community guidelines: Security Policy, Contributing guide, and Issue/PR templates
- Dual-licensing structure (MIT and Apache 2.0)
