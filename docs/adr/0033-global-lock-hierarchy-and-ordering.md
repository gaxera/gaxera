# ADR 0033: Global Lock Hierarchy and Mechanical Ordering Enforcement

> **Status:** Approved  
> **Date:** 2026-07-25  
> **Initiative:** Core Kernel Lock Safety & Deadlock Prevention  
> **Applies To:** `kernel` (`kernel/src/global.rs`)  

---

## Context & Problem Statement

Gaxera multi-core execution relies on global spinlocks for resource accounting (`RESOURCE_DOMAINS`), capability tree derivation (`CAPABILITY_SYSTEM`), object generation tracking (`OBJECT_ARENA`), physical frame allocation (`PHYSICAL_ALLOCATOR`), and typed object registries (`ENDPOINTS`, `ADDRESS_SPACES`, `MEMORY_OBJECTS`, etc.).

Acquiring multiple spinlocks out of order across CPU cores introduces classic AB-BA deadlock hazards. While a 5-level lock hierarchy was previously outlined in code comments, there was zero mechanical enforcement, no type-level rank tracking, and no runtime assertions verifying acquisition order.

---

## Architectural Invariants

> **Invariant 1 — Strict Monotonic Rank Ordering:**  
> A CPU core currently holding a spinlock at rank `Level N` may ONLY acquire additional locks at rank `Level M` where `M > N`.
> 
> **Invariant 2 — Registry Non-Nesting Invariant:**  
> Parallel `Level 4` registry locks (`ENDPOINTS`, `ADDRESS_SPACES`, `MEMORY_OBJECTS`, `CAPABILITY_SPACES`, etc.) MUST NEVER be nested together. A thread holding one Level 4 registry lock must release it before acquiring another Level 4 lock.
> 
> **Invariant 3 — Mechanical Debug-Mode Enforcement:**  
> Lock acquisitions evaluate the currently held lock rank. Attempting to acquire a lock out of order triggers a `debug_assert!` failure in debug and test builds.
> 
> **Invariant 4 — Zero Lock State across User Boundaries:**  
> Global kernel locks must never be held across context switches, user copy routines, thread scheduling, or device I/O operations.

---

## Technical Decisions

### 1. Lock Hierarchy Levels
The kernel defines 5 total lock ranks (Level 0 through Level 4):
- **Level 0:** `RESOURCE_DOMAINS` (Resource domain quota management)
- **Level 1:** `CAPABILITY_SYSTEM` (Global capability lineage and derivation tree)
- **Level 2:** `OBJECT_ARENA` (Object slot allocation and generation tracking)
- **Level 3:** `PHYSICAL_ALLOCATOR` (Physical frame bitmap allocator)
- **Level 4:** Typed Object Registries (`ENDPOINTS`, `ADDRESS_SPACES`, `CAPABILITY_SPACES`, `MEMORY_OBJECTS`, `DEBUG_CONSOLES`, `FACTORIES`, `WAIT_SETS`, `NOTIFICATIONS`, `INTERRUPTS`, `MAPPINGS`, `CONTIGUOUS_FRAMES`)

### 2. `RankedLock<T, const LEVEL: u8>` Abstraction
All global spinlocks in `kernel/src/global.rs` are wrapped in `RankedLock<T, const LEVEL: u8>`, which encapsulates a inner `spinning_top::Spinlock<T>` alongside const rank metadata `LEVEL`.

```rust
pub struct RankedLock<T, const LEVEL: u8> {
    inner: Spinlock<T>,
}
```

### 3. Per-CPU Acquired Rank Tracking
During `RankedLock::lock()`, the implementation:
1. Verifies that `LEVEL` is strictly greater than the CPU's current highest held lock rank (or `LEVEL == 4` with no other Level 4 lock held).
2. Updates the per-CPU highest held lock rank guard.
3. Automatically restores the previous held lock rank when the returned `RankedLockGuard` is dropped.

---

## Consequences

1. **Deadlock Elimination:** Out-of-order lock acquisitions are caught at test/debug time before reaching production hardware.
2. **Type-System Clarity:** Lock ranks are explicitly visible in type signatures and global definitions in `kernel/src/global.rs`.
3. **Zero Cost in Release Mode:** Debug rank tracking assertions compile away in release builds, retaining raw `spinning_top::Spinlock` performance.
