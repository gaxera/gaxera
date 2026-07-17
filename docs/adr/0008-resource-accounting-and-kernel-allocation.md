# ADR 0008: Resource Accounting and Kernel Allocation

**Status:** Accepted
**Date:** 2026-07-17
**Deciders:** Gaxera project

## Context

v0.1 has a fixed kernel heap whose fatal allocation path is acceptable during
bootstrap diagnostics but cannot be exposed to user-triggerable object or
memory creation. The former ten-object architecture has no owner for the
lifetime, accounting, and delegation of allocation authority. Attaching that
authority to a capability space, address space, or scheduler context would
conflate independently managed lifetimes.

## Decision

`ResourceDomain` is the eleventh first-class kernel object. It owns bounded
allocation authority and charges created kernel objects and anonymous memory
to a domain. It is deliberately distinct from `AddressSpace`,
`CapabilitySpace`, and `SchedulingContext`.

A Factory is not a twelfth object. It is a capability right on a
`ResourceDomain` that permits a bounded set of creation operations. The right
can be delegated only with narrower type and capacity limits. v0.5 starts with
simple fixed limits and accounting counters; it does not claim hierarchical
budgets, leases, memory-pressure policy, or complete denial-of-service
protection.

Kernel object storage is private, typed, and fallible. It may use arenas or
slabs backed by kernel-managed memory, but semantic object identity is stable
and never a storage address. Every user-triggerable creation, growth, mapping,
or transfer preparation path returns a defined exhaustion error rather than
calling an infallible allocator or panicking.

Boot-time failure before mandatory memory, allocator, and initial-domain setup
remains terminal. Once the kernel is ready to create or run user work,
resource exhaustion is recoverable. Interrupt, exception, and scheduler paths
remain allocation-free unless a later ADR explicitly changes that invariant.

The initial init manifest receives a bounded ResourceDomain Factory right, not
raw physical memory, untyped memory, arbitrary frame allocation, page-table
mutation, APIC, port-I/O, or unrestricted debug authority. v0.5 exposes only
kernel-created anonymous `MemoryObject`s and read-only boot-payload memory.

## Consequences

The Technical Specification's object model is amended from ten to eleven
objects. The addition removes a structural ambiguity that would otherwise
force hidden global allocation authority or incorrectly couple accounting to
another object. A future resource policy can evolve behind ResourceDomain
without changing ordinary object or address-space ownership.

M1 must prove allocation exhaustion, accounting non-underflow, and Factory
rights denial in host tests. This ADR does not define physical-memory or
untyped-memory capabilities, DMA accounting, global OOM recovery, fair-share
scheduling, or a public resource-management policy.

## Alternatives Considered

**A global Factory capability:** rejected because it is ambient authority by
another name and cannot provide meaningful accounting.

**Quotas on CapabilitySpace or AddressSpace:** rejected because capability,
mapping, and resource lifetimes are not identical.

**PhysicalMemory or UntypedMemory capabilities in v0.5:** rejected because
they add retyping, zeroing, physical-address disclosure, revocation, and DMA
obligations before their consumers exist.

**Retaining ten objects with implicit accounting:** rejected because hidden
allocation authority cannot be reviewed, delegated, or verified.
