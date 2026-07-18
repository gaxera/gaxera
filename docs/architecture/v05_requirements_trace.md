# Gaxera v0.5 Requirements Trace

> **Status:** Canonical release-scope trace
> **Architecture baseline:** `docs/architecture/foundation_v0.1.md`
> **Release program:** `docs/roadmap/roadmap_v05.md`
> **Rule:** This document stages long-term commitments; it does not silently
> amend them. A conflicting change requires an ADR.

## Purpose

The Technical Specification describes Gaxera's long-term system. v0.5 is a
smaller verified microkernel release, not a claim that all long-term mechanisms
already exist. This trace prevents a long-term commitment from being mistaken
for a v0.5 exit criterion and records the architectural representation left
for later work.

| Requirement | v0.5 disposition | Evidence or successor |
| --- | --- | --- |
| Minimal microkernel mechanism | Implement | Ring-3, capabilities, address spaces, IPC, timer proof, and services; no kernel filesystem or drivers. |
| Capability authority and revocation | Implement bounded core | ADR 0007; stale-handle, rights, transfer, and revocation tests. |
| Resource budgets | Implement bounded foundation | ADR 0008 `ResourceDomain`; fixed accounting and fallible exhaustion, not global OOM policy. |
| Eleven-object model | Represent incrementally | ADR 0008 amends KRN-02. v0.5 implements only objects with accepted consumers; InterruptObject remains deferred. |
| User address-space isolation | Implement | M2A/M2B mappings, guard pages, W^X, and hostile-pointer proofs. |
| User-memory copying | Implement only after recovery contract | ADR 0009; register-only first ABI and fault-recoverable copies. |
| Shared memory for bulk IPC | Implement minimal explicit mapping | `MemoryObject` capability mapping, not arbitrary pointer IPC. |
| Multi-class scheduler, EEVDF, affinity, SMP | Deferred | M5 proves one BSP preemptive scheduler only. |
| ASLR/KASLR, demand paging, compression, global OOM policy | Deferred | v0.5 fixed layout and bounded resource limits. |
| Leases and policy-driven expiry | Deferred | ResourceDomain and capability derivation preserve a future ownership boundary. |
| Init supervision and restart | Implement bounded proof | M6 controlled crash/restart under a limited initial authority manifest. |
| Filesystem and shell | Implement user-space demonstrator | Ramfs parses opaque payload data; a deterministic scripted session replaces interactive input. |
| Secure boot and signed payloads | Deferred | ADR 0014 uses reproducibly built developer-trusted static init only. |
| Device authority and interactive input | Deferred | Output-only bootstrap console; no port-I/O or IRQ capability in v0.5. |
| Physical/untyped memory capabilities | Deferred | Anonymous and read-only boot `MemoryObject`s only; ADR 0008. |

## Accepted v0.5 Architecture Changes

- ADR 0007 defines generational handles, bounded derivation, and immediate
  future-use revocation semantics.
- ADR 0009 isolates recoverable user-copy faults from terminal kernel faults.
- ADR 0008 introduces `ResourceDomain` as the eleventh kernel object and makes
  post-bootstrap user-triggerable allocation permanently fallible.
- ADR 0010 separates the internal M2A `iretq` privilege-transition proof from
  the later public syscall ABI and assigns GDT/TSS/`RSP0` ownership to the
  architecture layer.
- ADR 0011 requires a distinct user CR3 root with supervisor-only shared
  kernel mappings and a fixed, W^X M2A probe layout.
- ADR 0013 defines the contracts for synchronous endpoint IPC, notifications,
  and strict all-or-nothing capability transfer semantics, isolating them from
  scheduling policy.
- ADR 0014 remains the single future decision point for static init artifacts,
  payload manifest, and boot-payload loading. ADR 0017 is not scheduled.

## Release Boundary

v0.5 demonstrates integrity and isolation for the stated UEFI QEMU scenarios.
It does not claim resource-availability guarantees under hostile load,
physical-hardware validation, authenticated boot, or a complete device model.
