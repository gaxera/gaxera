# ADR 0039: Ring-3 Heap Foundation Scope and Bootstrap Boundary

**Status:** Accepted  
**Date:** 2026-08-01  
**Deciders:** Gaxera Core Maintainers  
**Related Architecture:** `docs/architecture/userspace_runtime.md`  
**Supersedes:** The allocator-implementation and release-scope portions of ADR 0038

## Context

ADR 0038 defined the required safety contracts for capability-mediated
anonymous memory, ResourceDomain byte accounting, reference-class reclamation,
and a fallible Ring-3 allocator. It also described a future allocator shape
with slab/buddy bins, direct large allocations, and guard-page decommitment.

The v1.1 implementation has now established and verified the foundational
contracts, but its concrete allocator is intentionally smaller: it manages
fixed 64 KiB mapped chunks through an out-of-band bounded block table. The
repository does not yet have a generic process loader that provisions Factory
and AddressSpace capabilities to every production service. Migrating service
entrypoints before that bootstrap contract exists would make their startup
authority implicit and untestable.

The release must describe the verified mechanism rather than claim future
allocator policy or service integration that does not exist.

## Decision

1. **v1.1.0 closes the Ring-3 memory foundation.** The release includes:
   - ResourceDomain physical-byte accounting;
   - Factory type authorization and narrow MemoryObject rights;
   - page-rounded, zero-filled, non-executable anonymous memory;
   - transactional physical-frame rollback;
   - capability, mapping, and transient reference classes;
   - mapping lineage and selective revocation;
   - a fallible `UserspaceAllocator` backed by fixed 64 KiB virtual chunks;
   - genuine Ring-3 lifecycle and heap QEMU verification.

2. **The current allocator is a bounded chunk free-list, not a complete
   slab/buddy allocator.** `HeapArena` stores out-of-band `BlockMeta` entries,
   grows in 64 KiB chunks, returns null on exhaustion, and defers coalescing
   until a fitting free block cannot be found. It does not yet provide
   size-class slab bins, direct large-allocation mappings, page decommitment,
   or inter-chunk guard pages.

3. **Allocator initialization is explicit.** A Ring-3 process must call
   `UserspaceAllocator::init(factory, address_space)` before using dynamic
   allocation. The dedicated test process receives the initial handles in
   slots 0 and 3, matching the existing bootstrap convention. An
   uninitialized allocator returns null rather than acquiring ambient memory
   authority.

5. **FactoryCreate wire encoding is preserved explicitly.** The live
   dispatcher uses `rsi = 0` for FactoryCreate, while public
   `OperationCode::Call = 2` is endpoint IPC. This is a compatibility quirk
   retained for v1.1.0; a future ABI revision may assign FactoryCreate a
   distinct named opcode, but no current wrapper may silently encode it as
   `OperationCode::Call`.

6. **Production-service migration is deferred.** Replacing the remaining
   service dummy allocators is not part of the v1.1.0 release until a generic
   process-bootstrap contract specifies how the loader provisions Factory,
   AddressSpace, CapabilitySpace, and failure-reporting authority. The work is
   tracked as a follow-up to the v1.1 memory foundation, not silently treated
   as complete.

7. **The IRQ architecture remains independent.** `docs/architecture/irq_delivery.md`
   remains a Draft and no v1.1.0 claim is made that hardware IRQ delivery to a
   Ring-3 driver is complete.

## Consequences

The v1.1.0 evidence is honest and reproducible: it proves the memory mechanism
and its lifecycle without pretending that unbootstrapped production services
already own heap authority. Future work can replace the bounded arena behind
the `GlobalAlloc` contract or add guard/decommit policy without changing the
kernel MemoryObject ownership model.

The cost is that `Box` and `Vec` support is currently verified in dedicated
Ring-3 test services rather than in every production server. A future process
bootstrap ADR and loader implementation must precede service migration.

## Rejected Alternatives

* **Blindly replace every dummy allocator now:** Rejected because it would
  invent Factory and AddressSpace handle ownership for services that are not
  currently loaded by the bootstrap path.
* **Claim the ADR 0038 future allocator shape is implemented:** Rejected
  because the current code has no slab bins, direct large-allocation path,
  guard-page insertion, or decommit protocol.
* **Add a compound heap-growth syscall:** Rejected; the existing
  factory-create, map, and compensating-delete sequence is sufficient for the
  verified foundation and keeps policy in Ring 3.
