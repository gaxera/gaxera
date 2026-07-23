# ADR 0021: First-Class WaitSet & Multi-Client IPC Architecture

> **Status:** Accepted  
> **Date:** 2026-07-23  
> **Initiative:** Initiative #2 — IPC Evolution Architecture (`docs/architecture/ipc.md`)  
> **Applies To:** `gaxera-abi`, `kernel-core`, `kernel`  

---

## Context & Problem Statement

In Gaxera v0.5, `Endpoint` IPC was strictly $1:1$ synchronous rendezvous. An `Endpoint` could hold at most one pending caller thread and one waiting receiver thread. If a second caller attempted `OperationCode::Call` on a busy server endpoint, the call was rejected immediately with `EndpointError::Busy`. Furthermore, server processes could not wait on multiple endpoints, notifications, or timers simultaneously.

To support multi-client microkernel services (such as filesystems, window managers, and network servers), Gaxera requires:
1. Multi-client caller queueing on server endpoints.
2. Multiplexed waiting ($N$-source event waiting) across endpoints, notifications, and timers.
3. Priority inheritance to prevent unbound priority inversion across IPC call chains.

---

## Decision

We adopt **First-Class `WaitSet` Kernel Objects & Bounded Multi-Client Endpoint Queueing**:

1. **Bounded `Endpoint` Caller Queues:**
   - Extend `Endpoint` to maintain a bounded FIFO wait-queue (`CallerQueue`) holding up to 32 caller entries (`CallerEntry { caller: ObjectId, message: InlineMessage }`).
   - If a caller invokes an endpoint when the queue is full, the call returns `ResourceLimitExceeded`.

2. **First-Class `WaitSet` Kernel Object (`ObjectType::WaitSet = 13`):**
   - Introduce `WaitSet` as a kernel object that aggregates event readiness signals from registered capabilities (`Endpoint`, `Notification`, `Timer`).
   - Add new ABI opcodes:
     - `OperationCode::CreateWaitSet = 10`
     - `OperationCode::WaitSetControl = 11` (ADD / REMOVE subscriptions with a `user_cookie: u64` and `signals: u32` mask)
     - `OperationCode::WaitSetWait = 12` (atomic multi-source wait)

3. **IPC Priority Inheritance:**
   - When a high-priority caller calls a lower-priority server, the server thread's effective priority is dynamically boosted to match the caller's priority until `OperationCode::Reply` completes.

---

## Rationale & Alternatives Considered

### Alternative 1: Single-Caller Rendezvous (v0.5 Baseline) — REJECTED
* **Pros:** Zero caller queue memory overhead, trivial state machine.
* **Cons:** Single-threaded server limitations; cannot scale to real microkernel workloads where multiple processes access the same service.

### Alternative 2: Badged Notification Binding (seL4 Style) — REJECTED
* **Pros:** Reuses existing `Endpoint` and `Notification` types without introducing a new `WaitSet` object.
* **Cons:** Limited flexibility when polling disparate object types (e.g. timers + endpoints + device notifications); requires complex badge bit-mapping in userspace.

---

## Consequences & Invariants

1. **Zero Fast-Path Allocations:** Event notifications within `WaitSet` use preallocated slab-cached event slots.
2. **Deterministic Teardown:** Destroying an `Endpoint` or `WaitSet` atomically invalidates cross-object subscriptions and unblocks pending callers with `EndpointError::Closed`.
