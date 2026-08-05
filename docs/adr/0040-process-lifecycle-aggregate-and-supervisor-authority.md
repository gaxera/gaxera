# ADR-0040: Process Lifecycle Aggregate and Supervisor Authority

**Status:** Accepted
**Date:** 2026-08-03
**Amends:** None

## Context

Prior to v1.2.0, the Gaxera microkernel managed isolation primitives (`Thread`, `AddressSpace`, `CapabilitySpace`, `ResourceDomain`) independently. However, these primitives lacked a unifying lifecycle abstraction. This resulted in implicit and fragmented semantics for driver crash containment, resource cleanup, and restart policies, forcing the kernel to make assumptions that belong in userspace policy. Without a formal process aggregate, we cannot deterministically cleanly tear down a failed component or safely grant a supervisor authority over its lifecycle without granting ambient access to its raw components.

## Decision

We introduce a first-class `Process` object (`ObjectType::Process = 15`) to serve as a lifecycle/control aggregate.
1. **Ownership**: The Process object explicitly records references to its child `AddressSpace`, `CapabilitySpace`, main `Thread`, `ResourceDomain`, and bootstrap manifest. It does not subsume their responsibilities (e.g., `MemoryObject` still owns frames, `AddressSpace` still owns page tables).
2. **State Machine**: The Process implements a strict state machine: `New -> Prepared -> Runnable -> Running -> ExitRequested -> Exiting -> Zombie -> Reaped`.
3. **Supervisor Authority**: A userspace supervisor (e.g., `init`) provisions the process, starts it, handles its exit Notification, and must explicitly `Reap` the zombie process.
4. **Terminal States**: `Dead -> Runnable` resurrection is removed. Crashed services must be restarted by creating a completely fresh Process identity and fresh primitives.

## Consequences

- **Easier**: Driver crash containment, process teardown, and resource reclamation become deterministic and structurally guaranteed.
- **Easier**: Supervisor authority is explicitly modeled via the `Process` capability, rather than requiring raw unconstrained capability injection.
- **Harder**: Process creation is no longer a loose assembly of primitives; it requires a transactional ABI operation (`CreateProcess`) that rolls back completely on any intermediate failure.

## Alternatives Considered

- **Implicit Process via Root Thread**: We considered designating a "main thread" as the de facto process root, but this conflated execution scheduling with structural resource lifecycle, breaking the microkernel capability principle of separation of concerns.
- **POSIX `fork`/`exec`**: Rejected immediately as incompatible with Gaxera's deterministic, capability-oriented architecture.
