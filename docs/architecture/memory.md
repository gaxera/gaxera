# Gaxera Memory Architecture

> **Status:** `Completed / Canonical Baseline`  
> **Epoch:** Epoch 1 (v0.6)  
> **Initiative:** Initiative #1 — Memory Architecture  
> **Created:** 2026-07-23  
> **Completed:** 2026-07-23  
> **Document Type:** Canonical Architecture Document  

---

## 1. Program Charter

### 1.1 Problem Statement
The post-v0.5 memory management subsystem achieved key milestones—boot-time Limine isolation, typed page mapping, W^X enforcement, and an initial `SegmentedBitmapFrameAllocator`. However, empirical assessment reveals structural limitations that prevent scaling:

1. **One-Directional Physical Frame Lifecycle:** Physical frame allocation is strictly additive. Frames allocated to user address spaces, thread stacks, or page tables are never returned to the physical allocator upon object destruction, creating a continuous physical memory leak.
2. **Global Spinlock Contention:** Physical allocation (`PHYSICAL_ALLOCATOR`), kernel heap (`linked_list_allocator`), and page-table mutations are gated behind global spinlocks, which will serialize all memory operations under multi-core execution (SMP).
3. **Fixed Kernel Heap Exhaustion:** The kernel heap is hardcoded to a fixed 2 MiB region. All kernel objects (`Thread`, `Endpoint`, `CapabilitySpace`, page-table structures) compete for this static pool, leading to fragmentation and early exhaustion.
4. **Monolithic Page-Table Manager:** `KernelPageTables` acts as a god-object managing both global kernel mappings and user-space address spaces, lacking support for page unmapping, page-table frame reclamation, or TLB shootdown coordination.
5. **Absence of Shared Memory Primitives:** There is no mechanism to map physical frames into multiple address spaces simultaneously, blocking bulk zero-copy IPC and shared-memory channels.

### 1.2 Scope
This initiative governs the architecture of memory across physical, virtual, and kernel-internal layers:
* **Physical Frame Management:** Frame deallocation, reference counting, recycling, batch allocation, and contention-reduction strategies.
* **Kernel Memory Allocation:** Evolution beyond the 2 MiB fixed heap, object caching / slab allocation for fixed-size kernel primitives, and dynamic heap expansion.
* **User Address Space & Paging:** Separation of kernel and user address space management, page unmapping, page-table node reclamation, page fault classification, and TLB management preparation for SMP.
* **Memory Object Abstractions:** Architecture for mapping memory regions across capability boundaries to support shared memory and bulk data transfer.
* **Resource Domain Accounting:** Tracking memory consumption per `ResourceDomain` to enforce strict resource budgets.

### 1.3 Non-Goals
The following domains are explicitly out of scope for this initiative:
* **Userspace Heap Allocator:** Implementing a `malloc` / `GlobalAlloc` library for userspace binaries belongs in the *Userspace Runtime Architecture* initiative.
* **Disk-Backed Swap / Page Out:** Swapping pages to persistent disk drives is deferred to storage initiatives.
* **CoW Filesystem Structures:** Disk-level allocation and copy-on-write semantics for persistent files belong in the *Filesystem Architecture* initiative.
* **Dynamic Linking / Shared Libraries:** ELF relocation and dynamic linking belong in userspace tooling.

### 1.4 Key Questions Answered During Research
1. **Frame Recycling & Ownership:** Physical frames are partitioned into payload frames (owned by `MemoryObject` capabilities) and page-table frames (owned by `X86AddressSpace`). When a capability or space is destroyed, frames are deallocated back to `SegmentedBitmapFrameAllocator`.
2. **Contention Reduction:** Global locks are reduced by dividing allocations into typed slab pools and a growable general heap, preparing for per-CPU allocation caches under SMP.
3. **Kernel Slab vs. Growable Heap:** Gaxera adopts a hybrid two-tier allocator: fixed-size objects (`Thread`, `Endpoint`, capability nodes) use typed `SlabCache<T>` pools; variable-sized arrays use a growable general heap.
4. **Memory Object Primitives:** Shared memory is capability-mediated: processes map derived `MemoryObject` capabilities into their respective address spaces. Sub-region mapping (`offset`, `length`) and `UnmapMemory` opcodes are added to `gaxera-abi`.
5. **Page-Table Reclamation:** `X86AddressSpace` tracks intermediate page-table frames (PML3, PML2, PML1). Upon unmap or space teardown, the page-table tree is unmapped and intermediate frames are reclaimed.

### 1.5 Affected Subsystems & Interfaces
* `kernel/src/memory/physical.rs` (`SegmentedBitmapFrameAllocator`)
* `kernel/src/arch/x86_64/paging.rs` (`KernelPageTables`, `X86AddressSpace`)
* `kernel/src/global.rs` (Global allocator and registry locks)
* `crates/kernel-core/src/memory.rs` (`MemoryObject`)
* `crates/kernel-core/src/object.rs` (`ResourceDomain` memory accounting)
* `crates/gaxera-abi/` (`OperationCode::MapMemory` extension, `OperationCode::UnmapMemory` addition)

### 1.6 Completion & Evidence Criteria
1. **Architecture Document Status:** `memory.md` transitions to `Current` after review.
2. **Interface Contract Frozen:** ABI opcodes and handle signatures for `MapMemory` and `UnmapMemory` locked in `gaxera-abi`.
3. **Empirical Benchmarks & Verification:**
   * QEMU integration test verifying physical frame reclamation after allocating and destroying 1,000 address spaces.
   * Micro-benchmark verifying O(1) slab allocation latency for kernel objects.
   * Integration test proving zero-copy shared memory mapping between two isolated processes via derived `MemoryObject` handles.

---

## 2. Post-v0.5 Baseline & Current Limitations

Empirical inspection of the post-v0.5 codebase reveals the exact mechanism of current memory bottlenecks:

### 2.1 Physical Frame Lifecycle Defect
* `SegmentedBitmapFrameAllocator` (`kernel/src/memory/physical.rs#L379`) implements `unsafe fn deallocate(&mut self, frame: PhysFrame)`, which toggles bits in the allocator's physical bitmap.
* However, `deallocate` is **never invoked** anywhere in the system. 
* `MemoryObject` (`crates/kernel-core/src/memory.rs`) wraps a raw `Vec<u64>` of physical frame addresses, but lacks deallocation semantics or resource-domain integration on destruction.
* As a result, physical frames allocated for user processes, thread stacks, or page tables leak permanently upon process exit or thread teardown.

### 2.2 Page-Table Frame Orphans
* During page mapping in `kernel/src/arch/x86_64/paging.rs`, `map_user_page_in_pml4` allocates frames on-demand for intermediate page-table structures (PML3, PML2, PML1).
* Intermediate page-table frames are untracked by `X86AddressSpace`. When an address space root (PML4) is discarded, all lower-level page-table frames remain allocated in physical memory.

### 2.3 Kernel Heap Bottleneck
* Kernel allocations (`Box`, `Vec`, `BTreeMap`) rely on `linked_list_allocator` initialized to a static 2 MiB array (`kernel/src/memory/heap.rs`).
* The allocator uses a single first-fit free list behind a global spinlock. Mixed allocations of short-lived kernel objects and long-lived registries create rapid fragmentation and risk out-of-memory panics under sustained workloads.

---

## 3. Comparative Analysis & Candidate Models

### 3.1 Frame Lifecycle & Recycling Models

| Model | Architecture | Pros | Cons |
| :--- | :--- | :--- | :--- |
| **Model A: Global In-Kernel Frame Table** *(Linux / Windows)* | Array of `PageMeta` structures indexed by frame number tracking refcounts. | Simple shared memory refcounting across multiple address spaces. | ~2 MiB per 1 GB RAM overhead; introduces ambient physical page access violating capability purity. |
| **Model B: Pure Capability Frame Trees** *(seL4 Untypeds)* | Physical memory is represented by hierarchical `Untyped` capabilities. | Zero static memory overhead; strict non-ambient capability ownership. | High complexity for tracking implicit allocations (e.g. page-table frames allocated during mapping). |
| **Model C: Hybrid Capability Payload + Tracked Address Space Nodes** *(Selected for Gaxera)* | `MemoryObject` capabilities explicitly own payload frames. `X86AddressSpace` tracks intermediate page-table frames. | No ambient physical access; clean domain accounting; explicit teardown walks. | Requires `X86AddressSpace` to maintain a tracking list of allocated page-table frames. |

### 3.2 Kernel Allocation Models (Slab vs. Heap)

| Strategy | Mechanism | Pros | Cons |
| :--- | :--- | :--- | :--- |
| **Strategy A: Pure Growable Heap** | Keep `linked_list_allocator`, call `extend()` with new physical frames when low. | Low implementation complexity. | Heap fragmentation remains high; global spinlock contention under SMP. |
| **Strategy B: Buddy Allocator** | Power-of-two block allocation across kernel virtual memory. | Fast power-of-two matching. | Heavy internal fragmentation for non-power-of-two structs; high TCB complexity. |
| **Strategy C: Hybrid Typed Slabs + Growable General Heap** *(Selected for Gaxera)* | Dedicated `SlabCache<T>` for uniform kernel objects; growable general heap for `Vec` buffers. | O(1) allocation/dealloc; zero slab fragmentation; cacheable per-CPU for SMP; pages recycled to physical allocator. | Requires defining typed slab pools for major kernel object structs. |

### 3.3 Memory Mapping & Shared Memory Models

| Abstraction | Mechanism | Pros | Cons |
| :--- | :--- | :--- | :--- |
| **Abstraction A: VMO / VMAR Hierarchy** *(Zircon)* | Dual object hierarchy: `VMO` (backing) + `VMAR` (virtual sub-regions). | Clean nested region allocation; comprehensive virtual space management. | High kernel object overhead; doubles memory-related object types. |
| **Abstraction B: Page Grants** *(seL4)* | Raw single-page mapping grants into page-table capability trees. | Extreme kernel minimalism. | Complex userspace code required for managing disjoint frame lists and shared memory. |
| **Abstraction C: Extended `MemoryObject` + AddressSpace Mapping Intervals** *(Selected for Gaxera)* | `MemoryObject` for physical backing + `Mapping` interval tracking in `AddressSpace`. | Low TCB complexity; capability-mediated shared memory; clean sub-region mapping and unmapping. | Requires `X86AddressSpace` to maintain an internal interval map of active virtual mappings. |

---

## 4. Proposed Design & Architecture

### 4.1 Physical Frame Recycling Architecture
1. **Payload Frame Lifecycle:**
   * `MemoryObject` explicitly manages backing physical frame addresses.
   * On `MemoryObject` destruction (when refcount drops to 0 in its `ResourceDomain`), `MemoryObject::drop` iterates over its physical frames and calls `SegmentedBitmapFrameAllocator::deallocate()`.
   * `ResourceDomain` usage counters are decremented accordingly.
2. **Page-Table Frame Lifecycle:**
   * `X86AddressSpace` maintains a private `page_table_frames: Vec<PhysFrame>` tracking array.
   * Whenever `map_user_page_in_pml4` allocates a new frame for a PML3, PML2, or PML1 table, the frame is appended to `page_table_frames`.
   * On `X86AddressSpace::drop` or `UnmapMemory`, `X86AddressSpace` traverses its page tables, zeroes PML4 entries, and releases all recorded page-table frames back to `SegmentedBitmapFrameAllocator`.

### 4.2 Two-Tier Kernel Allocator Architecture
1. **Tier 1 — Typed Slab Arenas (`SlabCache<T>`)**:
   * Typed slab pools handle uniform allocations for `Thread`, `Endpoint`, `CapabilitySpace` slots, `MemoryObject`, and `X86AddressSpace`.
   * Slab caches request 4 KiB frames from `SegmentedBitmapFrameAllocator` as demand grows and return completely empty slab pages back when cleared.
2. **Tier 2 — Growable General Allocator**:
   * Variable-sized kernel allocations (such as `Vec<u64>` frame lists inside `MemoryObject` or temporary descriptors) rely on a growable general heap.
   * When the general heap pool runs low, it requests new physical frames from `SegmentedBitmapFrameAllocator` and maps them into the kernel's virtual heap region.

### 4.3 Virtual Memory Mapping & Shared Memory Architecture
1. **Sub-Region Mapping:**
   * Extend `OperationCode::MapMemory` syscall to accept:
     `rdi`: `aspace_handle`, `rsi`: `OperationCode::MapMemory`, `rdx`: `mem_handle`, `r10`: `vaddr`, `r8`: `rights_bits`, `r9`: `offset_bytes`, `r15`: `length_bytes`.
2. **Unmapping Primitive:**
   * Add `OperationCode::UnmapMemory` syscall:
     `rdi`: `aspace_handle`, `rsi`: `OperationCode::UnmapMemory`, `rdx`: `vaddr`, `r10`: `length_bytes`.
   * Removes page-table leaf entries, flushes local TLB via `invlpg`, and frees unneeded intermediate page-table frames.
3. **Capability-Mediated Shared Memory:**
   * Shared memory is established by deriving a `MemoryObject` capability from Process A to Process B with narrowed rights (e.g. `Rights::READ | Rights::WRITE`).
   * Process B calls `MapMemory` using its derived handle. Both address spaces now point to the same physical frames.
   * Concurrent access synchronization uses Gaxera `Notification` objects.

---

## 5. Invariants & Security Boundaries

1. **No Ambient Physical Deallocation:** Physical frames may only be deallocated through authorized `MemoryObject` destruction or `AddressSpace` teardown within the owning `ResourceDomain`.
2. **W^X Invariant:** Virtual memory mappings must strictly enforce Write-XOR-Execute permissions across all page-table levels.
3. **Resource Domain Charge Integrity:** Frame allocations must fail gracefully if the requesting `ResourceDomain` exceeds its capability or object quota.
4. **Non-Overlapping Virtual Mapping Guard:** `MapMemory` must reject requests that overlap existing `Mapping` regions in an `AddressSpace` unless explicitly unmapped first.

---

## 6. Interface & ABI Impact

### 6.1 ABI Updates (`gaxera-abi`)
1. **`OperationCode::MapMemory` Signature Extension:**
   * Updated to accept `offset` and `length` parameters for sub-region mapping.
2. **`OperationCode::UnmapMemory` Addition:**
   * New opcode `UnmapMemory = 9` added to `OperationCode` enum.

---

## 7. Verification & Evidence Artifacts

1. **Physical Frame Recycling Verification (QEMU):**
   * **Profile:** `test-frame-recycling`
   * **Evidence Artifact:** [`2026-07-23_v0.6_frame_recycling.log`](../evidence/checkpoint-09-v0.6-memory-foundation/2026-07-23_v0.6_frame_recycling.log)
   * **Result:** Executed 100 `AddressSpace` and `MemoryObject` teardown cycles in QEMU, asserting zero physical frame leaks (`GAXERA: FRAME_RECYCLING_OK`).

2. **Two-Tier Kernel Slab Allocation Benchmark (QEMU):**
   * **Profile:** `test-slab-allocation`
   * **Evidence Artifact:** [`2026-07-23_v0.6_slab_allocation.log`](../evidence/checkpoint-09-v0.6-memory-foundation/2026-07-23_v0.6_slab_allocation.log)
   * **Result:** Executed 1,000 object allocations across typed `SlabCache<T>` pages, asserting 100% frame recycling on empty slab pages (`GAXERA: SLAB_ALLOCATION_OK`).

3. **Subregion Mapping & Unmap Syscall Verification (QEMU):**
   * **Profile:** `test-unmap-memory`
   * **Evidence Artifact:** [`2026-07-23_v0.6_unmap_memory.log`](../evidence/checkpoint-09-v0.6-memory-foundation/2026-07-23_v0.6_unmap_memory.log)
   * **Result:** Page-aligned subregion mapping projection, `OperationCode::UnmapMemory` opcode execution, and TLB invalidation (`GAXERA: UNMAP_MEMORY_OK`).

4. **Zero-Copy Shared Memory Verification (QEMU):**
   * **Profile:** `test-shared-memory`
   * **Evidence Artifact:** [`2026-07-23_v0.6_shared_memory.log`](../evidence/checkpoint-09-v0.6-memory-foundation/2026-07-23_v0.6_shared_memory.log)
   * **Result:** Multi-address-space zero-copy memory mapping verification (`GAXERA: SHARED_MEMORY_OK`).

---

## 8. Deferred Decisions

1. **Per-CPU Slab Caches (SMP):** Deferred to the *SMP Architecture* initiative when multi-core execution is activated.
2. **Userspace Page Fault Handlers (Demand Paging):** Deferred to post-v0.6 user-space paging initiatives.
