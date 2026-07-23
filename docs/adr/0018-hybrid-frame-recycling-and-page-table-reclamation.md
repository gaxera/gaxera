# ADR 0018: Hybrid Frame Recycling and Page-Table Reclamation

## Status
Accepted

## Context
In post-v0.5 Gaxera, `SegmentedBitmapFrameAllocator` possessed an `unsafe fn deallocate(&mut self, frame: PhysFrame)` method, but it was never invoked. Payload frames allocated to user processes, thread stacks, and intermediate page-table structures (PML3, PML2, PML1) were orphaned upon process exit or space destruction, causing a continuous physical memory leak.

Evaluating alternatives:
- **Global In-Kernel Frame Table (Linux/Windows Model):** Allocates a global array of `PageMeta` structs tracking refcounts for every 4 KiB frame. Rejected because it incurs ~2 MiB per 1 GB RAM overhead and introduces ambient physical page lookups, violating capability purity.
- **Pure Capability Frame Trees (seL4 Untyped Model):** Tracks every frame via capability lineage. Rejected due to high kernel complexity when managing intermediate page-table frames allocated implicitly during page mapping.

## Decision
We adopt a **Hybrid Frame Recycling Model**:
1. **Payload Frames:** Owned by `MemoryObject` capabilities. When a `MemoryObject` reference count drops to 0 or its `ResourceDomain` is destroyed, `MemoryObject::drop` deallocates its backing physical frames back to `SegmentedBitmapFrameAllocator`.
2. **Page-Table Frames:** Owned directly by `X86AddressSpace`. `X86AddressSpace` maintains a private tracking vector (`page_table_frames: Vec<PhysFrame>`) for intermediate page-table nodes allocated during page mapping. On space teardown or `UnmapMemory`, `X86AddressSpace` traverses its tree, zeroes entries, and releases page-table frames to `SegmentedBitmapFrameAllocator`.

## Consequences
- **Positive:**
  - Prevents physical memory leaks on process exit and thread stack destruction.
  - Preserves capability purity: physical frames are never accessed by raw ambient physical addresses.
  - Decouples payload frame lifetime from implicit page-table node lifetime.
- **Negative:**
  - `X86AddressSpace` must allocate and maintain a private tracking list for allocated page-table frames.
