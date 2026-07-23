# Gaxera v0.6 Epoch Roadmap: Core Memory Foundation

> **Status:** Completed  
> **Baseline:** Gaxera v0.5, tag `v0.5.0`  
> **Target:** Gaxera v0.6.0  
> **Primary Initiative:** Memory Architecture (`docs/architecture/memory.md`)  
> **ADR References:** ADR 0018, ADR 0019, ADR 0020  
> **Completion Date:** 2026-07-23  

---

## 1. Executive Direction

v0.6 establishes the memory infrastructure required for Gaxera to run multi-client services without leaking memory or fragmenting the kernel heap.

Where v0.5 proved ring 3 execution, capability-mediated syscalls, and basic rendezvous IPC in closed-loop payloads, v0.6 turns memory management into a production-grade, non-leaking, capability-governed foundation.

### Key Objectives
1. **Physical Frame Recycling:** Zero physical memory leaks on address space, memory object, or thread stack teardown (`ADR 0018`). [COMPLETED]
2. **Two-Tier Kernel Allocator:** Replace static 2 MiB linked-list heap fragmentation with O(1) typed slab arenas (`SlabCache<T>`) and a growable general heap (`ADR 0019`). [COMPLETED]
3. **Sub-Region Mapping & Unmapping:** Extend `MapMemory` to support `(offset, length)` slices and introduce the `UnmapMemory` opcode (`ADR 0020`). [COMPLETED]
4. **Capability Shared Memory:** Enable zero-copy shared memory between isolated tasks via derived `MemoryObject` capabilities. [COMPLETED]

---

## 2. Milestone Structure & Acceptance Criteria

### Milestone 0.6.1: Physical Frame Recycling & Page-Table Reclamation
* **Architecture Reference:** `docs/architecture/memory.md#41`, `ADR 0018`
* **Status:** `Complete`
* **Deliverables:**
  * Implement `MemoryObject::take_frames` payload frame deallocation back to `SegmentedBitmapFrameAllocator`.
  * Track page-table frames inside `X86AddressSpace` and implement recursive frame reclamation (`destroy_user_pml4`) on space teardown.
* **Acceptance Criterion:** QEMU integration test `test-frame-recycling` allocates 100 address spaces with mappings, destroys them, and verifies physical frame count returns to 100% of the pre-test baseline. (PASSED)

### Milestone 0.6.2: Two-Tier Kernel Slab Arenas & Heap Evolution
* **Architecture Reference:** `docs/architecture/memory.md#42`, `ADR 0019`
* **Status:** `Complete`
* **Deliverables:**
  * Implement `SlabCache<T>` for uniform kernel objects (`Thread`, `Endpoint`, `CapabilitySpace` slots, `MemoryObject`, and `X86AddressSpace`).
  * Convert `kernel/src/memory/heap.rs` to growable general heap backed by `SegmentedBitmapFrameAllocator`.
* **Acceptance Criterion:** Micro-benchmark proves O(1) object allocation latency and zero slab fragmentation across 1,000 object allocations. (PASSED)

### Milestone 0.6.3: Subregion Memory Mapping & Unmap Primitive
* **Architecture Reference:** `docs/architecture/memory.md#43`, `ADR 0020`
* **Status:** `Complete`
* **Deliverables:**
  * Update `OperationCode::MapMemory` and `MemoryObject::frames_subrange` to validate `offset` and `length`.
  * Implement `OperationCode::UnmapMemory` (`Opcode 9`) syscall handler, page-table clearing, `invlpg` TLB flush, and page-table frame deallocation.
* **Acceptance Criterion:** Integration test profile `test-unmap-memory` verifies subregion mapping and page unmapping correctness. (PASSED)

### Milestone 0.6.4: Zero-Copy Shared Memory Verification
* **Architecture Reference:** `docs/architecture/memory.md#43`, `ADR 0020`
* **Status:** `Complete`
* **Deliverables:**
  * Implement shared-memory test binary pair demonstrating zero-copy communication over derived `MemoryObject` handles mapped into separate address spaces.
* **Acceptance Criterion:** Integration test profile `test-shared-memory` verifies bidirectional zero-copy reads/writes between two isolated address spaces. (PASSED)

---

## 3. Non-Goals Enforced in v0.6
* SMP multi-core execution (deferred to v0.8+).
* Disk-backed swapping or page-out to physical disk drives.
* Multi-class EEVDF scheduler (scheduler evolution belongs to a future initiative).
