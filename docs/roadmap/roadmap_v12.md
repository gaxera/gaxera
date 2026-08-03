# Initiative v1.2.0 Roadmap — Device Event and Ring-3 Runtime Completion

> **Status:** Proposed
> **Epoch:** v1.2
> **Target Release:** v1.2.0

## 1. Initiative Charter

Gaxera v1.2.0 carries forward the deferred work from the original broader v1.1 scope. While v1.1.0 successfully delivered the Ring-3 Memory Foundation, it did not complete production-service allocator migration or hardware IRQ delivery because their fundamental prerequisites—process bootstrap authority and lifecycle semantics—did not yet exist.

Initiative v1.2.0 delivers this missing architecture, resolving how processes are created, authorized, supervised, and torn down, thereby allowing safe migration of production services to the new Ring-3 memory model and establishing a capability-scoped driver notification system.

## 2. Objectives

- Design and implement the Process Bootstrap Architecture, defining process creation, authority handoff, and resource ownership.
- Complete process and resource teardown (`ExitProcess` semantics and lifecycle cleanup).
- Migrate the `init` service and subsequent production services to the fallible Ring-3 heap without relying on dummy allocators.
- Deliver end-to-end capability-scoped hardware IRQ delivery and Notification semantics.
- Define device driver lifecycle and crash containment.

## 3. Non-Goals

- v1.2 does not begin with blind production-service allocator replacement; bootstrap architecture is a strict prerequisite.
- No AP bring-up, physical SMP execution, or runtime lock enforcement (reserved for v1.3).
- No durable persistent data or NVMe targets (reserved for v1.4).
- No physical reference platform claims.
- No IOMMU or DMA isolation features.

## 4. Research Gates

- **RG-6 (System Continuity and Trust Lifecycle):** Must resolve process startup capability set, supervisor relationship, and failure reporting before production services migrate.
- **RG-1 (Interrupt Delivery and Notification Semantics):** Must define IRQ vector ownership, ACK/mask/rearm behavior, and teardown rules before driver implementation.

## 5. Milestones

### M1 — Process Bootstrap Architecture
Define process creation authority, startup capability set, Factory ownership, AddressSpace ownership, CapabilitySpace ownership, Thread ownership, ResourceDomain charging, supervisor relationship, startup metadata, and failure reporting. (No code should be described as complete here).

### M2 — Bootstrap Capability Handoff
Implement and verify explicit startup provisioning.
**Required evidence:** Ring-3 process receives declared handles; handle meaning is not guessed from slot numbers; unauthorized authority is absent; bootstrap metadata is validated.

### M3 — Process and Resource Lifecycle
Define and implement `ExitProcess` teardown, mapping cleanup, MemoryObject cleanup, AddressSpace cleanup, Thread cleanup, ResourceDomain lifetime, delegated memory after creator exit, supervisor revocation, and crash cleanup.

### M4 — Production Service Allocator Migration
Migrate services individually. Required sequence:
1. `init`
2. service registry/bootstrap services
3. driver services
4. network/storage services

**Each migration must include:** explicit allocator initialization, fallible allocation behavior, untrusted-input audit, quota failure behavior, crash/restart behavior, and evidence. (Do not remove dummy allocators globally in one change).

### M5 — IRQ Delivery
Complete IOAPIC/MSI/MSI-X routing, vector ownership, `InterruptObject` capabilities, Notification binding, WaitNotification/WaitSet wakeup, ACK/mask/unmask/rearm semantics, spurious vectors, teardown after driver exit, and vector reuse protection.

### M6 — Driver Lifecycle
Define discovery grant, startup, notification loop, crash containment, resource reclamation, reset, rebind, and service restart policy.

### M7 — v1.2 Verification and Closeout
Require host tests, deterministic QEMU tests, malformed capability tests, allocator exhaustion tests, IRQ notification tests, service crash tests, teardown/reclamation tests, documentation synchronization, and immutable release evidence.

## 6. Dependencies

- No production service migration before bootstrap authority exists.
- No interrupt-driven driver before IRQ ownership and notification semantics exist.

## 7. Ownership Boundaries and Unsafe Invariants

- **INV-PROC-01:** Startup capability handles must be explicitly provisioned via defined metadata; no slot-number assumptions.
- **INV-IRQ-01:** Hardware interrupts must map to capability-mediated `InterruptObject`s; unprivileged processes cannot listen to arbitrary IRQs.

## 8. Verification Matrix & Evidence Requirements

See Milestone M7. Proof requires explicit QEMU matrix tests demonstrating bounded failure, capability isolation, and crash containment.

## 9. Explicit Future Work

- Physical Execution and Reference Platform (v1.3.0)
- Persistent Data and First Physical Storage (v1.4.0)
