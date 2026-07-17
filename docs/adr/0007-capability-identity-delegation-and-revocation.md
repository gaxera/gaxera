# ADR 0007: Capability Identity, Delegation, and Revocation

**Status:** Accepted
**Date:** 2026-07-17
**Deciders:** Gaxera project

## Context

v0.5 introduces the first authority-bearing kernel interfaces. A handle must
not be a kernel address or a merely descriptive identifier: it must name a
validated authority with a bounded lifetime, type, rights, and delegation
history. The design must reject stale handles, support narrowed delegation and
atomic transfer, and revoke delegated authority without an unbounded syscall
walk.

## Decision

Gaxera uses a per-domain capability space of generational slots. The user ABI
exposes only opaque handles encoding a slot and generation; a handle never
contains a kernel pointer. Slot lookup validates generation, object type,
rights, object liveness, and derivation validity before returning authority.

Each capability slot has a derivation node. Derivation creates a child with a
strict subset of the parent's rights and records a bounded parent lineage.
Each node carries revocation state. `revoke` invalidates its subtree
logically: once it returns, every future validation of the capability or any
descendant fails. Validation combines slot generation with bounded ancestry
and revocation-generation checks. Physical cleanup of invalid descendants may
occur later at bounded safe points.

The following operations are distinct:

- deleting a handle removes that slot's authority without necessarily
  invalidating independently derived descendants;
- revoking a handle invalidates that handle's derivation subtree;
- destroying an object invalidates every capability lookup for that object
  once its object-lifetime rules permit destruction.

Capability transfer is a prepare/commit/rollback transaction. A failed
transfer leaves both capability spaces unchanged. Rights may only narrow in a
derivation or transfer. There is no raw-object-pointer escape from a successful
lookup.

An already accepted kernel operation is not retroactively cancelled by a later
revocation. Revocation blocks future authorization. IPC-specific cancellation
and reply-authority rules are deferred to ADR 0013.

## Consequences

The capability core has small, host-testable state transitions and immediate
authorization semantics without unbounded descendant traversal. The price is
bounded lineage metadata, validation work proportional to the configured
maximum lineage depth, and explicit cleanup work. The exact limit is an
implementation constant covered by state-machine tests, not a user ABI value.

Generational slots solve stale handle reuse. They do not alone solve delegated
revocation, which is why derivation state is retained separately. This ADR does
not define resource allocation, user ABI registers, IPC cancellation, leases,
or multi-level CSpaces.

## Alternatives Considered

**Generational slots only:** rejected because a valid derived capability would
survive parent revocation.

**Eager recursive revocation:** rejected as the primary mechanism because an
attacker-controlled capability tree could make revocation unbounded.

**Purely lazy revocation with eventual enforcement:** rejected because a
revoked capability must fail every future authorization immediately.

**Raw pointers or flat integer object IDs:** rejected because they conflate
identity and authority and make stale-reference errors too easy.
