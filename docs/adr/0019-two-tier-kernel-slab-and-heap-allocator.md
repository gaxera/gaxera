# ADR 0019: Two-Tier Kernel Slab and Heap Allocator

## Status
Accepted

## Context
In post-v0.5 Gaxera, kernel heap allocations (`Box`, `Vec`, `BTreeMap` in object registries) relied exclusively on a fixed 2 MiB `linked_list_allocator::LockedHeap`. Mixing short-lived temporary descriptors with long-lived kernel objects (`Thread`, `Endpoint`, `CapabilitySpace` slots) in a single first-fit list caused severe memory fragmentation, global spinlock contention, and early heap exhaustion.

Evaluating alternatives:
- **Pure Growable Heap:** Calling `HEAP.lock().extend()` using physical frames when space runs low. Rejected because it does not solve fragmentation from mixed allocation sizes or lock contention under SMP.
- **Buddy Allocator:** Power-of-two block allocator. Rejected due to high internal fragmentation for non-power-of-two kernel structs and unnecessary TCB complexity.

## Decision
We adopt a **Two-Tier Kernel Allocator Architecture**:
1. **Tier 1 — Typed Slab Arenas (`SlabCache<T>`):** Uniform-sized kernel objects (`Thread`, `Endpoint`, `CapabilitySpace` slots, `MemoryObject`, `X86AddressSpace`) are allocated from dedicated typed slab pools. Slabs request 4 KiB frames from `SegmentedBitmapFrameAllocator` on demand and return empty slab pages when cleared.
2. **Tier 2 — Growable General Allocator:** Variable-length allocations (`Vec<u64>` frame lists, temporary descriptors) use a growable general heap that dynamically maps additional physical frames when capacity is reached.

## Consequences
- **Positive:**
  - O(1) allocation and deallocation latency for core kernel objects.
  - Zero fragmentation for slab-managed kernel objects.
  - Slabs prepare naturally for per-CPU local caches (`PerCpuSlab<T>`) under future SMP activation.
- **Negative:**
  - Requires defining and initializing distinct slab caches for each kernel object type.
