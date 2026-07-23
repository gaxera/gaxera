# ADR 0020: Subregion Memory Mapping and Unmap Primitive

## Status
Accepted

## Context
In post-v0.5 Gaxera, `OperationCode::MapMemory` mapped an entire `MemoryObject` into an `AddressSpace` starting at a given virtual address. There was no mechanism to map a sub-region `(offset, length)` of a memory object, no `UnmapMemory` opcode, and no virtual address collision checking in `X86AddressSpace`.

Evaluating alternatives:
- **Two-Level VMO/VMAR Hierarchy (Zircon Model):** Dual object hierarchy (`VMO` for physical backing, `VMAR` for virtual sub-regions). Rejected because it doubles memory-related object types and increases microkernel TCB overhead.
- **Untyped Page Grants (seL4 Model):** Mapping raw single pages directly. Rejected due to high userspace complexity for managing disjoint frame lists.

## Decision
We extend `MemoryObject` capabilities and `X86AddressSpace`:
1. **Sub-Region Mapping:** Extend `OperationCode::MapMemory` ABI to accept `offset` and `length` parameters.
2. **Unmapping Primitive:** Introduce `OperationCode::UnmapMemory` (`Opcode 9`) accepting `(aspace_handle, vaddr, length)`.
3. **Virtual Mapping Intervals:** `X86AddressSpace` maintains an internal `Mapping` interval tracker to prevent overlapping virtual mappings and ensure proper cleanup during unmapping.
4. **Shared Memory via Capability Derivation:** Processes establish zero-copy shared memory by deriving `MemoryObject` capabilities to other tasks via standard IPC and calling `MapMemory`.

## Consequences
- **Positive:**
  - Enables flexible memory mapping (sub-slices of ELF images, shared buffers).
  - Provides a clean `UnmapMemory` syscall with page-table frame reclamation.
  - Enables zero-copy shared-memory IPC without introducing new kernel objects.
- **Negative:**
  - `X86AddressSpace` must maintain an internal mapping interval structure.
