# Ring-3 Userspace Runtime & Memory Allocation Architecture

> **Status:** Current  
> **Initiative:** v1.1.0 — Device Event & Ring-3 Runtime Foundation  
> **Scope:** Ring-3 User Heap Allocator, Memory Capabilities, Process Memory Lifecycle  
> **Related ADRs:** ADR 0008, ADR 0018, ADR 0019, ADR 0020, ADR 0024, ADR 0037, ADR 0038, ADR 0039

---

## 1. Program Charter

### 1.1 Problem Statement
At the `v1.0.0` baseline, the Ring-3 global allocator returned `null_mut()`
for every allocation request. The v1.1.0 foundation closes that mechanism gap
for processes that explicitly initialize `UserspaceAllocator`; production
service adoption remains a separate bootstrap task.

To support dynamic event buffering, packet queues, and service workloads in Initiative `v1.1`, Gaxera requires a fallible, capability-mediated Ring-3 user heap runtime backed by kernel `MemoryObject` capabilities.

### 1.2 Non-Goals
* **Kernel Monolithic Heap Expansion:** The kernel heap (`kernel_core::memory`) remains strictly separate and is not accessible to Ring 3.
* **Implicit Ambient Page Growth:** Processes do not receive automatic page allocation on page faults (`brk`/`sbrk` style implicit growth is explicitly rejected). All memory growth must be explicit and capability-backed.
* **Arbitrary Physical Page Pinning:** Ring-3 processes cannot request arbitrary physical frame addresses. Physical allocation remains mediated by kernel capabilities and `ResourceDomain` bounds.
* **Kernel Monolithic Driver Heap:** Driver memory allocations execute inside Ring-3 process space, not in Ring 0.
* **Unevidenced Compound Kernel Opcodes:** Heap expansion uses standard syscall primitives with compensating runtime transactions rather than inventing new compound kernel opcodes without evidence.

### 1.3 Key Questions Answered by This Architecture
1. What are the specific implementation gaps between current `v1.0.0` code and intended memory capabilities?
2. How is allocation authority mediated separately from `ResourceDomain` quota accounting?
3. How is `MemoryObject` lifetime managed across process exit, capability delegation, and supervisor revocation?
4. What is the constrained anonymous-memory contract governing heap allocations?
5. What is the complete Ring-3 virtual address space layout?
6. How does the Ring-3 runtime execute compensating transactions across separate kernel syscalls?
7. How does Rust `no_std` OOM behavior behave on our pinned toolchain for `alloc()`, `Vec::push`, and `try_reserve`?
8. How do different allocator designs evaluate against Gaxera requirements?
9. How are RG-1 (IRQ Delivery) and RG-2 (Ring-3 Allocator) decoupled for independent release?

### 1.4 Affected Interfaces
* `crates/gaxera-abi`: the current FactoryCreate wire operation is the
  dedicated `rsi = 0` suboperation; `OperationCode::Call` (`2`) remains the
  endpoint IPC operation. This legacy encoding is documented for compatibility
  and must not be confused with the public Call opcode.
* `crates/kernel-core`: `ResourceDomain` byte accounting (`memory_bytes`), `Factory` object mask checks, transactional frame allocation rollback.
* `crates/libgaxera`: `allocator.rs` (`UserspaceAllocator`), `heap.rs` (heap arena manager), `syscall.rs`.
* `crates/init` & Ring-3 servers: `#[global_allocator]` and `#[alloc_error_handler]` definitions.

### 1.5 Evidence Criteria for Completion
* **Host Unit Tests:** Arena free-list management, alignment, block splitting, coalescing, and OOM error returns.
* **QEMU Integration Tests:**
  1. `cargo xtask run --headless --test ring3-heap`: Genuine Ring-3 `Box` and `Vec` allocation/deallocation, alignment, fragmentation, quota exhaustion, and post-OOM recovery.
  2. `cargo xtask run --headless --test memory-lifecycle`: Real syscall-driven mapping, deletion, remapping, and clean process exit.
  3. `cargo xtask run --headless --test factory-correctness`: Factory authorization, page-rounded quota, narrow rights, and frame rollback.
  4. `cargo test --workspace --locked`: Host ABI, capability, ResourceDomain, mapping, and allocator tests.
  5. `cargo xtask test`: The complete locked verification matrix, including both Ring-3 profiles.

---

## 2. Historical Gap Analysis (v1.0.0 Baseline)

The following records the v1.0.0 starting point that motivated this
initiative. It is historical context, not a description of the current code:

1. **Syscall ABI Opcode Structure:**
   * In `gaxera-abi`, `OperationCode::Call` is ABI opcode 2 for endpoint IPC.
   * `factory_create` currently uses the dedicated `frame.rsi == 0` operation
     on a Factory capability. The distinction is a compatibility constraint;
     it is not encoded as `OperationCode::Call` in the live dispatcher.

2. **ResourceDomain Accounting Limits:**
   * `ResourceDomain` (`crates/kernel-core/src/resource.rs`) currently tracks only object count (`objects: u32`) and capability count (`capabilities: u32`).
   * It has zero physical frame or byte accounting (`memory_bytes`).

3. **Factory Type Authorization Enforcement Gap:**
   * In `syscall.rs` lines 1140–1230, `factory_create` checks `Rights::FACTORY` on the handle, but does **not** evaluate `factory.allows(obj_type)`. Any valid `Factory` handle can currently create any `ObjectType`.

4. **Non-Transactional Partial Frame Allocation Failure:**
   * In `syscall.rs` lines 1176–1193, `factory_create` allocates physical frames in a loop (`for _ in 0..num_frames`). If physical allocation fails mid-loop, the kernel breaks with `u64::MAX` without freeing frames already allocated in that loop, leaking physical memory.

5. **Over-Privileged Root Capability Rights:**
   * In `syscall.rs` line 1243, new objects created via `factory_create` are inserted into the caller's `CapabilitySpace` with `gaxera_abi::Rights::ALL`.

6. **Lack of Shared Reference Counting & Delegated Lifetime Model:**
   * `MemoryObject` physical frames are stored in an un-refcounted vector. When a process exits, its address space unmaps local page tables, but derived handles or shared mappings in other processes do not have defined reference-counting semantics.

7. **Architectural Contradiction with ADR 0008:**
   * ADR 0008 stated: *"A Factory is a capability right on ResourceDomain, not another object."*
   * The codebase implements `ObjectType::Factory = 12` as a standalone kernel object in `OBJECT_ARENA` and `FACTORIES` registry.
   * This architectural disagreement requires explicit reconciliation.

---

## 3. v1.1.0 Implemented Foundation

The current implementation resolves the historical gaps above. The kernel now
charges page-rounded physical bytes to the Factory's ResourceDomain, enforces
Factory type masks before allocation, returns narrow MemoryObject rights,
zeroes frames before publication, and rolls back partial frame allocation and
quota charges. MemoryObject lifecycle is protected by capability, mapping, and
transient reference classes; Mapping objects retain capability lineage so
revocation can selectively unmap descendants.

`libgaxera::UserspaceAllocator` is verified as a fallible bounded arena. It
uses fixed 64 KiB chunk growth, an out-of-band `BlockMeta` table, and explicit
Factory/AddressSpace initialization. Its deallocation path marks blocks free
and defers coalescing until allocation needs it, avoiding a quadratic free-path
scan under fragmentation. The allocator returns null on exhaustion and the
dedicated Ring-3 test covers recovery after a failed fallible reservation.

This is a foundation, not the complete future allocator design. Size-class
slabs, direct large allocations, guard-page insertion, page decommitment, and
ASLR remain deferred. Production services also remain on their existing
startup layouts until a generic capability bootstrap contract is accepted.

## 4. Architecture & Design Corrections

### 4.1 Factory Model & ADR 0008 Reconciliation
We preserve `ObjectType::Factory = 12` as an explicit kernel object. To reconcile with ADR 0008:
* The `Factory` **object** holds:
  1. `target_domain`: Target `ResourceDomainId` charged for created objects.
  2. `allowed_types`: An `ObjectTypeSet` mask restricting authorized `ObjectType` creations.
* `Rights::FACTORY` is a **capability handle right** present on the capability slot in `CapabilitySpace`, not a field on the `Factory` object itself.

Creating a `MemoryObject` requires presenting a capability handle that possesses `Rights::FACTORY` and references a `Factory` object whose `allowed_types` bitfield explicitly includes `ObjectType::MemoryObject`.

### 4.2 ResourceDomain Byte-Accounting Extension
We extend `ResourceDomain` (`crates/kernel-core/src/resource.rs`) to include physical byte quota tracking:

```rust
pub struct ResourceLimits {
    pub objects: u32,
    pub capabilities: u32,
    pub memory_bytes: u64,
}

pub struct ResourceUsage {
    pub objects: u32,
    pub capabilities: u32,
    pub memory_bytes: u64,
}
```

* **Charge:** `charge_memory(bytes)` checks `usage.memory_bytes + bytes <= limits.memory_bytes`.
* **Rollback:** `rollback_memory(bytes)` deducts `bytes` on partial allocation failure.
* **Release:** `release_memory(bytes)` refunds quota when a `MemoryObject` is destroyed.
* **Overflow:** Exceeding byte quota returns `ResourceError::MemoryLimit`.

#### Process Lifetime vs. ResourceDomain Lifetime
Process lifetime and `ResourceDomain` lifetime are distinct. If delegated memory remains charged to the creator's `ResourceDomain` after the creator process exits, that `ResourceDomain` must be independently owned by a supervisor process (or persist in the domain hierarchy) until all charges against it are released.

### 4.3 Factory Type Enforcement & Narrow Capability Handle
When `factory_create` produces a `MemoryObject`:
1. The kernel enforces `factory.allows(ObjectType::MemoryObject)`.
2. The root capability inserted into the caller's `CapabilitySpace` receives a narrow rights bitfield: `Rights::MAP | Rights::READ | Rights::WRITE`.
3. `Rights::ALL`, `Rights::EXECUTE`, `Rights::FACTORY`, and DMA authority are strictly excluded.

### 4.4 Mapping Lineage for Revocation Boundaries
To make `OperationCode::Revoke` deterministic, the kernel maintains an explicit **mapping lineage graph**:
* Every `Mapping` object / VMA created via `MapMemory` is recorded as a child descendant of the source `MemoryObject` capability.
* When `OperationCode::Revoke` is issued on the root `MemoryObject`, the kernel walks the mapping lineage graph, unmaps active VMAs across all consumer address spaces, invalidates all derived `CapabilitySpace` handles across all processes, drops reference counts to 0, and deallocates physical frames immediately.

### 4.5 Constrained Anonymous-Memory Contract
Anonymous heap memory mapped via `Factory` creation and `MapMemory` satisfies:
1. **Zero-Filled:** Physical frames are zero-filled via HHDM (`write_bytes(0)`) prior to mapping.
2. **Non-Executable:** Mapped with `Rights::READ | Rights::WRITE`. `Rights::EXECUTE` is rejected by `MapMemory` on anonymous memory objects. W^X is strictly enforced.
3. **Non-DMA:** Anonymous heap memory is unpinned, pageable virtual memory. It cannot be used for hardware DMA without an explicit `ContiguousFrame` capability with IOMMU translation.
4. **Page-Rounded & Bounded:** Sizes are rounded up to 4 KiB boundaries and bounded by `ResourceDomain.memory_bytes` quota.

---

## 5. Complete Ring-3 Virtual Address Space Layout

The 48-bit canonical user address space (`0x0000_0000_0000` to `0x0000_7FFF_FFFF_FFFF`) is partitioned as follows:

```
+-----------------------------------+ 0x0000_0000_0000
| Unmapped Guard Page / Null Trap   | 0x0000_0000_0000 - 0x0000_0000_0FFF (4 KiB)
+-----------------------------------+
| User Executable Segment (.text)   | 0x0000_0000_0040_0000 (ELF Base)
| User Read-Only Data (.rodata)     | R-X
| User Read-Write Data (.data/.bss) | R-- / RW-
+-----------------------------------+
| Reserved Executable/Library Space | 0x0000_1000_0000_0000 - 0x0000_3FFF_FFFF_FFFF
+-----------------------------------+
| Zero-Copy Shared Memory / IPC     | 0x0000_4000_0000_0000 - 0x0000_5FFF_FFFF_FFFF
| (IPC Ring Buffers / Mappings)     | RW- (Shared MemoryObjects)
+-----------------------------------+
| Ring-3 User Heap Region           | 0x0000_6000_0000_0000 - 0x0000_7BFF_FFFF_FFFF (~28 TiB)
|  - Lower Region Guard Page        | Unmapped (4 KiB)
|  - Dynamic Heap Arenas / Chunks   | RW- (Page-aligned, zeroed)
|  - Inter-Chunk Guard Pages        | Unmapped (4 KiB)
|  - Upper Region Guard Page        | Unmapped (4 KiB)
+-----------------------------------+
| User Stack Region                 | 0x0000_7C00_0000_0000 - 0x0000_7FFF_EFFF_FFFF
|  - Stack Guard Page               | Unmapped (Fault on overflow)
|  - Primary Thread Stack           | RW- (Fixed size e.g. 2 MiB)
+-----------------------------------+
| Canonical User High Limit         | 0x0000_7FFF_FFFF_F000 (USER_ADDRESS_MAX)
+-----------------------------------+ 0x0000_8000_0000_0000 (Kernel Space Start)
```

### Deterministic Reservation vs. Future ASLR
* **Initial Implementation:** Fixed, deterministic heap reservation starting at `0x0000_6000_0000_0000` for reproducible QEMU testing and debugging.
* **ASLR Status:** ASLR is designated as a future hardening feature requiring a cryptographically secure entropy source, image layout specification, and explicit reproducibility rules. Pseudo-random or unseeded QEMU placement is explicitly rejected as security.

The v1.1.0 test process grows contiguous 64 KiB chunks from this base. The
guard-page, shared-memory, stack, and library partitions shown above are target
address-space policy; only the explicit heap chunk mappings exercised by the
test service are implemented in this release.

---

## 6. MemoryObject Ownership, Reference Classes, and Lifecycle

Physical frames are released only when **all three reference classes** reach zero:

1. **Capability References:** Active capability handles pointing to the `MemoryObject` in any process's `CapabilitySpace`.
2. **Mapping References:** Active VMA page-table mappings referencing physical frames in any process's `AddressSpace`.
3. **Kernel Operation References:** Temporary kernel-internal references during active syscall execution.

Unmapping a VMA alone does **not** free a `MemoryObject` still held by a capability. Dropping a capability handle alone does **not** unmap or free physical memory still mapped in an active address space.

### Lifecycle Cases

* **Case A: Ordinary Process Exit (No Delegation)**
  Creator Process A exits. Process A's `CapabilitySpace` drops its handle; `AddressSpace` unmaps local VMAs. Both capability and mapping reference counts reach 0. Physical frames are deallocated to `SegmentedBitmapFrameAllocator`, and `ResourceDomain.memory_bytes` quota is fully refunded.

* **Case B: Creator Exit After Delegation**
  Creator Process A delegates a `MemoryObject` handle to Process B and then exits. Process A's handle is dropped and local VMAs unmapped, but Process B retains its valid capability handle and active VMA mapping. Capability and mapping reference counts remain $> 0$. Physical frames **remain allocated and accessible to Process B**. The frames remain charged to Creator's `ResourceDomain` (which must persist under supervisor ownership) until Process B releases its references.

* **Case C: Supervisor Revocation**
  Supervisor process calls `OperationCode::Revoke`. Kernel walks mapping lineage, unmaps active VMAs across all consumer address spaces, invalidates all derived `CapabilitySpace` handles, drops reference counts to 0, deallocates physical frames immediately, and refunds quota.

* **Case D: Final Reference Destruction**
  The final active capability handle is dropped **and** the final VMA mapping is unmapped. Reference counts across all classes reach 0. Physical frames are deallocated to `SegmentedBitmapFrameAllocator`, and quota is refunded to the `ResourceDomain`.

---

## 7. Ring-3 Compensating Transaction Protocol

Because the current FactoryCreate operation (`frame.rsi == 0`) and `MapMemory`
are separate kernel syscalls, the Ring-3 allocator cannot make the
cross-syscall sequence globally atomic. Instead, `libgaxera::UserspaceAllocator`
executes a **compensating transaction**:

```
[1. Quota Byte Charge] ──> [2. Frame Alloc & Zero] ──> [3. Arena Object Entry]
           │                         │                         │
      (Fail: Return)            (Fail: Refund)           (Fail: Free & Refund)
           │                         │                         │
           ▼                         ▼                         ▼
[6. Commit Allocator] <── [5. MapMemory VMA] <── [4. CSpace Slot Insert]
   (Success: 0)           (Fail: Unmap & Free)       (Fail: Remove & Free)
```

### Compensating Steps
1. **Kernel Syscall Atomicity:** Each individual kernel syscall (`factory_create`, `MapMemory`, `UnmapMemory`, `DeleteHandle`) is internally atomic in Ring 0.
2. **Runtime Rollback Recording:** Before each step, `libgaxera` records rollback state.
3. **Compensating Execution on Failure:**
   * If `factory_create` fails in step 2 (partial physical frame allocation): the kernel internally frees allocated frames, refunds quota, and returns `SyscallError::ResourceExhausted`.
   * If `MapMemory` fails in step 5 (virtual address collision or page table failure): `libgaxera` issues `DeleteHandle(mem_handle)`, releasing the kernel `MemoryObject` and refunding quota before returning `Err(AllocError)`.
4. **Process Exit Cleanup:** If a process crashes mid-transaction, standard process exit cleanup drops the temporary capability handle and unmaps partial VMAs.
5. **No Memory Leaks:** Zero unmapped frame leaks or partially published heap chunks remain visible to the allocator free-list.

---

## 8. Toolchain `no_std` Rust OOM Verification

### 8.1 Rust `GlobalAlloc` and `#[alloc_error_handler]`
In Rust `no_std`, `#[global_allocator]` delegates memory requests to `libgaxera::UserspaceAllocator`:
* `alloc(&self, layout: Layout) -> *mut u8`: Returns a valid page-aligned pointer on success, or `core::ptr::null_mut()` on failure.

When `alloc()` returns `null_mut()`:
* **Standard `Box::new` / `Vec::push`:** Invokes Rust's `#[alloc_error_handler]`. In `libgaxera`, `alloc_error_handler` calls `libgaxera::syscall::exit(EXIT_OOM_CODE)` (exit code `0xDEAD_0041`), cleanly terminating the process.
* **Fallible APIs (`try_reserve`, `Box::try_new`):** Catch `null_mut()` directly within the Rust `alloc` crate, returning `Err(TryReserveError::AllocError)` or `Err(AllocError)` **without** calling `alloc_error_handler` or aborting the process.

### 8.2 Verification Status
Host unit tests in `libgaxera` verify free-list alignment, splitting,
coalescing, and the uninitialized null result. The genuine Ring-3
`test_ring3_heap` service verifies quota/exhaustion behaviour through
`try_reserve_exact` and confirms that a subsequent small allocation still
works.

---

## 9. Requirement-Driven Allocator Selection

We evaluate three allocator architectures for `libgaxera`:

| Criterion | Option A: Gaxera Slab/Buddy Arena (Selected) | Option B: Ported `dlmalloc` | Option C: Bump Allocator |
| --- | --- | --- | --- |
| **Concurrency** | Thread-safe spinlock protecting bounded arena metadata. | Thread-safe via internal lock hooks. | Single-threaded only / coarse lock. |
| **Alignment** | Explicit alignment adjustment in out-of-band chunk metadata. | Custom alignment padding. | Alignment padding without recycling. |
| **Fragmentation** | Reusable free blocks with deferred coalescing. | Low general-purpose fragmentation. | Severe fragmentation (no dealloc). |
| **Metadata Placement** | Out-of-band arena descriptors (zero inline header corruption risk). | Inline boundary tags (vulnerable to buffer overflow). | Inline / zero metadata. |
| **Decommit Support** | Deferred; v1.1 retains mapped chunks until process teardown. | Partial trim support. | No decommit support. |
| **Fallible API Support** | Returns `null_mut()` cleanly on memory exhaustion. | Returns `null_mut()`. | Returns `null_mut()`. |
| **Testability** | 100% testable in `no_std` host unit tests (`cargo test`). | Complex C-ported dependency. | Trivial. |

**Selection:** The v1.1 foundation uses the bounded Gaxera chunk arena because
it keeps metadata out of user buffers, has a small auditable implementation,
and provides the required fallible contract. A future ADR may introduce
size-class slabs, direct large allocations, and decommitment once production
service workloads and bootstrap ownership are available.

---

## 10. Guard-Page Architecture (Future Hardening)

```
+-------------------------------------------------------+
| Permanent Region Guard (4 KiB Unmapped)               |
+-------------------------------------------------------+
| Heap Chunk 0 (Mapped RW-)                             |
+-------------------------------------------------------+
| Inter-Chunk Guard Page (4 KiB Unmapped)               |
+-------------------------------------------------------+
| Heap Chunk 1 (Mapped RW-)                             |
+-------------------------------------------------------+
| Standalone Large Allocation Guard (4 KiB Unmapped)    |
+-------------------------------------------------------+
| Large Allocation (>= 32 KiB Mapped RW-)               |
+-------------------------------------------------------+
| Standalone Large Allocation Guard (4 KiB Unmapped)    |
+-------------------------------------------------------+
| Permanent Region Guard (4 KiB Unmapped)               |
+-------------------------------------------------------+
```

The diagram is a target hardening policy, not a v1.1.0 implementation claim.
The current arena maps contiguous 64 KiB chunks without inter-chunk guard
pages and retains them until allocator/process teardown. Guard pages must be
added only together with an address-space allocation policy that can reserve
the gaps and test their fault semantics.

---

## 11. Decoupled Initiative Sequencing (RG-1 vs. RG-2)

Research Gate RG-2 (Ring-3 Allocator) and Research Gate RG-1 (Interrupt Delivery) are strictly decoupled:

* **RG-2 (Ring-3 Allocator):** Governed by this document and ADRs 0037–0039,
  tested via `cargo test --workspace --locked`, `factory-correctness`,
  `memory-lifecycle`, and `ring3-heap`.
* **RG-1 (IRQ Delivery):** Governed by `docs/architecture/irq_delivery.md`.
  It remains Draft and is not part of the v1.1.0 release evidence.

The two initiatives maintain separate architecture and verification gates.
RG-1 will receive its own implementation and release decision after the
bootstrap and notification contracts are complete.

---

## 12. Split Verification Matrix

| Verification Target | Test Scope & Scenario | Expected Result |
| --- | --- | --- |
| **No-Delegation Exit Reclamation** | Creator process allocates heap, frees nothing, calls `ExitProcess`. | 100% physical frames freed to `SegmentedBitmapFrameAllocator`; `ResourceDomain.memory_bytes` quota fully refunded. |
| **Delegated-Memory Survival** | Process A creates `MemoryObject`, delegates handle to Process B, then Process A exits. | Process B retains valid handle/mapping; frames **remain allocated**; Process A quota remains charged until Process B drops handle. |
| **Supervisor Revocation** | Supervisor calls `OperationCode::Revoke` on delegated `MemoryObject`. | 3-tier cascade revocation unmaps Process A and B page tables, invalidates handles, frees physical frames immediately. |
| **Partial-Allocation Rollback** | `factory_create` fails on frame $k$ of $N$ during physical allocation. | Frames $0..k-1$ freed, quota byte charge refunded, `SyscallError::ResourceExhausted` returned with zero frame leak. |
| **Byte-Quota Exhaustion** | Process requests allocation exceeding `ResourceDomain.limits.memory_bytes`. | Syscall returns `SyscallError::ResourceExhausted`; `alloc()` returns `null_mut()`; `try_reserve` returns `Err(...)`. |
| **Factory Type / Rights Denial** | Process presents `Factory` handle without `MemoryObject` in `authorized_types` or missing `Rights::FACTORY`. | Syscall fails with `SyscallError::RightsDenied` or `SyscallError::InvalidArgument`. |
