# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
