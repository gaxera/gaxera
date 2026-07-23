# ADR 0024: Userspace Runtime Architecture & `libgaxera`

> **Status:** Accepted  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.8.3 — Userspace Runtime (`docs/roadmap/roadmap_v08.md`)  
> **Applies To:** `libgaxera`, `init`, `crates/`  

---

## Context & Problem Statement

Currently, ring-3 applications in Gaxera (such as `init` and `script_session`) invoke raw assembly `syscall` instructions or ad-hoc wrappers directly. As Gaxera grows into a multi-service system, ring-3 binaries require:

1. A standard, safe userspace runtime library (`libgaxera`) providing idiomatic Rust abstractions over the raw Gaxera ABI.
2. Safe, ergonomic object wrappers for kernel primitives (`Endpoint`, `WaitSet`, `Notification`, `InterruptObject`, `MemoryObject`, `Mapping`).
3. Abstract freestanding global heap allocator interface (`GlobalAlloc`) for ring-3 applications operating in `#![no_std]` environment.
4. Standard runtime process entrypoint (`_start` lifecycle, `main` invocation, and clean process termination).

---

## Decision

We establish `libgaxera` (**Gaxera Userspace Runtime Library**, `crates/libgaxera`):

1. **Subsystem Isolation & Layered Architecture (`crates/libgaxera`):**
   - **Architecture Isolation:** Low-level inline assembly `syscall` instructions are strictly contained within architecture submodules (`libgaxera::arch::x86_64::raw_syscall`), exposing safe, platform-independent `raw_syscall0` through `raw_syscall6` functions to `libgaxera::syscall`.
   - **Crate Dependencies:** Depends exclusively on `gaxera-abi`. Does NOT depend on kernel-internal crates (`kernel-core` or `kernel`).

2. **Strict `OwnedHandle` & Object Wrapper Ownership Invariant:**
   - **`Handle` (`gaxera_abi::Handle`):** Lightweight, `Copy` capability handle reference for borrowed parameters without `Drop` side-effects.
   - **`OwnedHandle`:** Represents exclusive capability slot ownership (`!Copy`, `!Clone`). Executes `sys_delete_handle` on `Drop`.
   - **Uniform Object Wrapper Invariant:** `EndpointHandle`, `WaitSetHandle`, `NotificationHandle`, `InterruptHandle`, and `MappingHandle` internally own an `OwnedHandle`. Public methods expose borrowed `Handle` values transiently via `as_handle()`. Raw `Handle` values do not escape the public object API except where interoperability explicitly requires it.

3. **Modular Public Module Hierarchy & Prelude (`libgaxera::object`):**
   - Object wrappers are structured into specialized submodules:
     - `object::handle` (`OwnedHandle`)
     - `object::endpoint` (`EndpointHandle`)
     - `object::waitset` (`WaitSetHandle`)
     - `object::notification` (`NotificationHandle`)
     - `object::interrupt` (`InterruptHandle`)
     - `object::mapping` (`MappingHandle`)
     - `prelude` (convenience re-exports)

4. **Zero-Allocation Event Multiplexing (`WaitSetHandle`):**
   - `waitset.wait(&mut events: &mut [WaitSetEvent], timeout) -> Result<usize, SyscallError>` accepts a caller-provided event buffer, returning the count of triggered events without allocating heap memory.

5. **ABI Drift Protection & Compile-Time Verification (`libgaxera::abi_tests`):**
   - Compile-time static assertions verifying memory layout, alignment, and size invariants for `Handle`, `Message`, `TransferDescriptor`, and `WaitSetEvent`.
   - Static coverage checks for all `OperationCode` variants.

6. **Abstract Global Allocator Interface (`libgaxera::allocator`):**
   - Implements standard `core::alloc::GlobalAlloc` backed by user page-table mappings (`MapMemory`). The concrete allocator algorithm remains replaceable without altering runtime contracts.

7. **Runtime Process Entry & Lifecycle (`libgaxera::entry`):**
   - Provides standard entrypoint `#[no_mangle] pub extern "C" fn _start() -> !`.
   - Initializes runtime state, invokes user `main() -> i32` or `main()`, and terminates cleanly via `sys_exit(code)`.

---

## Rationale & Alternatives Considered

### Alternative 1: Monolithic `object.rs` File — REJECTED
* **Pros:** Single file.
* **Cons:** Hinders long-term modularity and clean API navigation as new kernel object types are added.

### Alternative 2: Raw Handle Leaks in High-Level API — REJECTED
* **Pros:** Simpler function signatures.
* **Cons:** Bypasses ownership model, exposing applications to double-drop and slot-leak risks.

---

## Consequences & Invariants

1. **Stable User-Space Compatibility Layer:** `libgaxera` serves as Gaxera's stable user-space compatibility abstraction layer. Future kernel ABI or syscall encoding changes must be absorbed within `libgaxera`, guaranteeing that ring-3 service code remains unchanged across kernel revisions.
2. **Move-Only Capability Ownership:** `OwnedHandle` and high-level wrappers prevent accidental double ownership or premature handle destruction.
3. **Zero Fast-Path Allocations:** Event multiplexing and syscall dispatching require zero heap allocations.
