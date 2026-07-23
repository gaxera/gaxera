# ADR 0023: MMIO Mapping Capabilities & Driver Address-Space Model

> **Status:** Accepted  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.8.2 — MMIO & Driver Foundation (`docs/roadmap/roadmap_v08.md`)  
> **Applies To:** `gaxera-abi`, `kernel-core`, `kernel`  

---

## Context & Problem Statement

In Gaxera's capability microkernel architecture, user-mode device drivers execute in isolated ring-3 address spaces without ambient root authority. To interact with physical hardware devices (such as LAPIC, IOAPIC, PCIe BAR MMIO windows, and NVMe controllers), drivers require:

1. A capability-mediated mechanism to map physical MMIO ranges into their address space without granting unrestricted physical memory access.
2. Architecture-neutral memory cache control attributes (`CachePolicy`) to configure page-level caching for hardware registers.
3. Subsystem separation consistent with ADR 0013: physical range authority must be encapsulated in a dedicated kernel object (`ObjectType::Mapping = 6`).

---

## Decision

We adopt **First-Class `ObjectType::Mapping` Capabilities & Capability-Mediated MMIO Mapping**:

1. **First-Class `Mapping` Kernel Capability (`ObjectType::Mapping = 6`):**
   - Represents a capability grant over a bounded physical address range `[phys_start, phys_start + size)`.
   - **Pure Range Metadata (Zero Allocation Invariant):** `Mapping` stores only `phys_addr: u64`, `size: usize`, and `cache_policy: CachePolicy`. It maintains **zero** physical frame vector allocations (`Vec<u64>`), preserving fixed-size kernel object memory footprint.
   - **Resource Ownership Invariant:** A `Mapping` capability grants authority to map an existing physical range into an address space. It does NOT allocate, reserve, or transfer ownership of the underlying physical memory/hardware resource.

2. **Architecture-Neutral Cache Policy (`CachePolicy`):**
   - Defined in `gaxera-abi` as an architecture-agnostic enum:
     ```rust
     #[repr(u8)]
     pub enum CachePolicy {
         Cached = 0,         // Standard RAM
         Uncached = 1,       // Strict MMIO
         WriteThrough = 2,   // Write Through
         WriteCombining = 3, // Write Combining (framebuffers)
     }
     ```
   - Target architecture layer (`kernel/src/arch/x86_64`) translates `CachePolicy` into platform-specific page-table bits (e.g. PCD, PWT, PAT) without exposing hardware details to `kernel-core`.

3. **Capability Authority vs. Requested Mapping Permissions:**
   - The `Mapping` capability handle carries granted rights (`capability.rights`: `Rights::READ`, `Rights::WRITE`, `Rights::MAP`).
   - `MapMemory` takes requested permissions (`requested_rights`). The kernel intersects requested rights with granted capability rights (`effective_rights = requested_rights & capability.rights`).
   - If `effective_rights` lacks `Rights::MAP`, the syscall fails with `CapabilityError::RightsDenied`.

4. **Mapping Alignment & Pre-Validation Invariants:**
   - `phys_addr` must be 4 KiB page-aligned (`phys_addr & 0xFFF == 0`).
   - `vaddr` must be 4 KiB page-aligned (`vaddr & 0xFFF == 0`).
   - `size` must be page-granular (`size > 0 && size & 0xFFF == 0`).
   - Virtual range must reside within canonical user-space (`vaddr + size <= USER_ADDRESS_MAX`).
   - Validation failures reject the syscall *before* taking address space locks or modifying page tables.

---

## Rationale & Alternatives Considered

### Alternative 1: Dynamic `Vec<u64>` in Mapping Object — REJECTED
* **Pros:** Reuses `MemoryObject` frame vector pattern.
* **Cons:** Requires dynamic heap allocations for contiguous physical ranges; breaks zero-allocation fast-path invariants.

### Alternative 2: Architecture-Specific `cache_disabled: bool` Flag — REJECTED
* **Pros:** Single boolean toggle.
* **Cons:** Not future-proof; cannot express Write-Combining or multi-level PAT modes across x86/ARM/RISC-V.

---

## Consequences & Invariants

1. **Strict Capability Boundary:** Unprivileged processes cannot map physical memory or MMIO ranges without possessing a valid `Mapping` capability handle.
2. **Zero Heap Allocations:** `Mapping` objects are pure metadata structs with fixed-size memory footprint.
3. **Atomic Lifecycles:** Unmapping an MMIO range removes page-table mappings and flushes TLB, ensuring subsequent access triggers a page fault.
