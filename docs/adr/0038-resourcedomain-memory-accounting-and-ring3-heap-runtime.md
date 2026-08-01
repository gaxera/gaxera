# ADR 0038: ResourceDomain Memory Accounting and Ring-3 Heap Runtime

**Status:** Accepted  
**Date:** 2026-07-30  
**Deciders:** Gaxera Core Maintainers  
**Related Architecture:** `docs/architecture/userspace_runtime.md`  

## Context

In Gaxera `v1.0.0`, the Ring-3 global allocator (`libgaxera::UserspaceAllocator`) returns `null_mut()` for all allocation requests. Furthermore, `ResourceDomain` tracks only object count and capability count, but has zero byte/frame accounting (`memory_bytes`). Partial physical frame allocation failures in `factory_create` leak memory without rollback, and `MemoryObject` creation inserts handles with over-privileged `Rights::ALL`.

To support dynamic user-space heaps in Initiative `v1.1`, Gaxera requires kernel memory byte-accounting, transactional failure rollback, constrained anonymous memory mapping, deterministic revocation lineage, and a fallible Ring-3 heap runtime in `libgaxera`.

## Decision

1. **ResourceDomain Byte Accounting:**
   `ResourceDomain` is extended to include physical byte quota limits (`limits.memory_bytes`) and usage tracking (`usage.memory_bytes`).
   * `charge_memory(bytes)`: Verifies `usage.memory_bytes + bytes <= limits.memory_bytes`.
   * `rollback_memory(bytes)`: Deducts `bytes` on partial frame allocation failures.
   * `release_memory(bytes)`: Refunds quota when physical frames are deallocated to `SegmentedBitmapFrameAllocator`.
   * Exceeding quota returns `ResourceError::MemoryLimit`.

2. **Process Lifetime vs. ResourceDomain Lifetime:**
   Process lifetime and `ResourceDomain` lifetime are distinct. If delegated memory remains charged to a creator process's `ResourceDomain` after that process exits, the `ResourceDomain` persists under supervisor ownership until all charges against it are released.

3. **Narrow MemoryObject Capability Rights & Constrained Contract:**
   `factory_create` produces a `MemoryObject` handle with narrow rights: `Rights::MAP | Rights::READ | Rights::WRITE`. `Rights::ALL`, `Rights::EXECUTE`, `Rights::FACTORY`, and DMA authority are strictly excluded.
   Anonymous heap memory mapped via `MapMemory` MUST be zero-filled by the kernel prior to mapping (`write_bytes(0)`), non-executable (W^X enforced), non-DMA (unpinned virtual pages), and page-aligned to 4 KiB boundaries.

4. **Reference Classes & Object Lifetime Semantics:**
   Physical frames are released only when **all three reference classes** reach zero:
   * **Capability References:** Active handles in any process's `CapabilitySpace`.
   * **Mapping References:** Active VMA mappings in any process's `AddressSpace`.
   * **Kernel Operation References:** Active kernel references during in-flight syscalls.

   Unmapping a VMA alone does not free a `MemoryObject` still held by a capability handle. Dropping a capability handle alone does not unmap or free physical memory still mapped into an active address space.

5. **Mapping Lineage & Deterministic Revocation:**
   The kernel tracks explicit **mapping lineage**: every `Mapping` / VMA is recorded as a child descendant of its source `MemoryObject` capability. `OperationCode::Revoke` walks this mapping lineage to unmap active VMAs across all consumer address spaces, invalidate derived capability handles, drop reference counts to 0, and deallocate physical frames immediately.

6. **Ring-3 Compensating Transaction Protocol:**
   `factory_create` (suboperation 0 inside `OperationCode::Call`) and `MapMemory` are separate kernel syscalls. `libgaxera::UserspaceAllocator` manages heap expansion using a **compensating transaction**:
   * Each kernel syscall is internally atomic in Ring 0.
   * `libgaxera` records rollback state before each step.
   * On runtime failure (e.g. `MapMemory` VMA collision), `libgaxera` executes compensating steps (`DeleteHandle`, releasing memory and quota) before returning `Err(AllocError)`.
   * Process exit cleans remaining local state; zero unmapped frame leaks or partially published heap chunks remain visible to the free-list.

7. **Libgaxera Heap Allocator Architecture:**
   `libgaxera::UserspaceAllocator` uses an out-of-band slab/buddy arena free-list design for allocations $< 32\text{ KiB}$ and direct `MemoryObject` mappings for allocations $\ge 32\text{ KiB}$. Empty slab pages map to `UnmapMemory` page decommitment.

8. **Toolchain OOM Behavior:**
   `alloc()` returning `null_mut()` triggers `#[alloc_error_handler]` (calling `exit(EXIT_OOM)`), while fallible APIs (`try_reserve`, `Box::try_new`) catch `null_mut()` and return `Err(...)` without process abort.

9. **Deterministic Virtual Reservation:**
   Initial implementation specifies a fixed deterministic heap reservation (`0x0000_6000_0000_0000`) enclosed by 4 KiB unmapped guard pages. ASLR is designated as a future hardening feature requiring an entropy source and layout policy.

## Consequences

* **Resource Safety:** Kernel allocation exhaustion returns `SyscallError::ResourceExhausted` without kernel panic or frame leaks.
* **Service Resilience:** Critical Ring-3 servers using fallible `try_reserve` APIs can handle memory pressure gracefully.
* **Isolation:** Ring-3 heap pages are strictly non-executable (W^X compliant) and isolated from DMA tampering.

## Alternatives Considered

* **Global Cross-Syscall Kernel Atomicity (Compound Opcode):** Rejected. Introducing compound kernel opcodes adds TCB complexity without empirical evidence. Compensating runtime transactions achieve clean failure recovery using existing atomic syscall primitives.
* **Inline Allocator Metadata (Boundary Tags):** Rejected. Inline headers are vulnerable to buffer-overflow corruption; out-of-band arena descriptors isolate allocator metadata.
