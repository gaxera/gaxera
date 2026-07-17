# ADR 0009: User-Memory Access and Fault Recovery

**Status:** Accepted
**Date:** 2026-07-17
**Deciders:** Gaxera project

## Context

v0.1 treats page faults as terminal diagnostic events. User-mode syscalls will
eventually need to access untrusted user-address-space buffers. Even after a
range check, a copy can fault because of an absent or changed mapping. Treating
that fault as an ordinary kernel fault would either terminate a recoverable
request or, worse, hide unrelated kernel corruption behind broad recovery.

## Decision

Before this contract is implemented, public v0.5 syscalls use only register
scalars and opaque handles. They accept no arbitrary user pointers. Bulk data
uses explicitly mapped `MemoryObject` pages rather than implicit pointer
copying.

When bounded user-copy operations are introduced, they must:

1. validate canonicality, user range, length, overflow, and access intent;
2. install one non-nestable per-CPU fault-resume record immediately before the
   specific faultable copy range;
3. clear that record on every normal and recovery return;
4. let the page-fault handler redirect only a matching kernel-mode copy fault
   to its recorded recovery label, which returns a defined user-access error;
5. leave every unrelated kernel page fault terminal and diagnostically intact.

No lock may be held across a recoverable copy unless its fault and recovery
behavior is independently specified. Interrupt and preemption policy must not
permit stale resume state to apply to a later fault. Future SMAP enablement
must be owned by the copy primitive; no caller may widen user-memory access by
leaving the architectural access state enabled.

## Consequences

The page-fault handler gains one narrowly documented recoverable path, while
the v0.1 terminal-fault contract remains the default. Register-only IPC keeps
the first syscall proof free of user-copy complexity. Explicit shared memory
makes bulk-data permissions and lifetimes visible in capabilities and mappings.

This ADR does not define demand paging, asynchronous page-in, pinning, user
fault handlers, copy-on-write, or a generic exception-resume mechanism.

## Alternatives Considered

**Trust range validation alone:** rejected because a valid range can still
fault at the hardware access.

**Recover every kernel page fault:** rejected because it masks kernel defects
and makes fault attribution unsafe.

**Arbitrary pointer-based IPC first:** rejected because it couples the first
IPC ABI to the most delicate exception path.

**Shared memory only:** rejected because control flow and capability transfer
still need a bounded syscall/endpoint control plane.
