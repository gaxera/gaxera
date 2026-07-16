# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
