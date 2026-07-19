# ADR 0016: Subsystem-Decomposed ELF Loader

## Status
Accepted

## Context
In Milestone 6, we introduced the capability to load and execute an ELF binary from the boot payload. The initial PoC combined parsing the ELF format, validating the headers, allocating physical memory, and modifying page tables into a single `elf.rs` implementation. This tightly coupled approach prevents unit testing the ELF parser on the host machine and mixes architecture-agnostic parsing with highly privileged architecture-specific mapping operations.

## Decision
We will completely decouple ELF processing into two distinct layers:
1.  **Parsing Subsystem (`kernel-core/elf/`)**: An architecture-agnostic, `#![no_std]`, zero-allocation parser that iterates over ELF headers and extracts information. It handles validation and typing but knows nothing of memory mapping or page tables.
2.  **Architecture Loader (`kernel/arch/x86_64/loader.rs`)**: An architecture-specific module that utilizes the parser subsystem to extract segments, allocate frames, map them into the `UserPageTables`, copy the data via the HHDM, and enforce `W^X` page protections.

## Consequences
-   **Positive:** 
    -   The ELF parser can be rigorously tested using `cargo test` on the host machine without relying on QEMU or architecture-specific stubs.
    -   Security is improved because the privileged operations (page table modifications) are isolated to `loader.rs` and cleanly separated from the complex parsing logic.
    -   Adding support for new architectures (e.g., aarch64) only requires a new `loader.rs`; the parser remains untouched.
-   **Negative:** 
    -   Slightly increased boilerplate as data structures are passed across the boundary between the core and the loader.
