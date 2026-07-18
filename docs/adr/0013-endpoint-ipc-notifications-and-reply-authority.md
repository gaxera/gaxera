# ADR 0013: Endpoint IPC, Notifications, and Reply Authority

**Status:** Accepted
**Date:** 2026-07-18
**Context:** Milestone M4 introduces the first mechanisms for user-level execution to coordinate synchronously, exchange events, and transfer capability authority. This document freezes the contracts for these mechanisms before any implementation begins, preventing ambiguous interactions or implicit policy from leaking into the core state machine.

## 1. Endpoint Contract

An endpoint in Gaxera is a strict, bounded synchronous rendezvous object. It is deliberately **not** an asynchronous queue, and it provides no buffering for messages that have not been received.

### 1.1 State and Rendezvous
- An endpoint holds at most **one pending caller**, **one waiting receiver**, and **one active reply authority**.
- There is deliberately no send-only endpoint operation, buffering policy, selective receive, cancellation, priority inheritance, or deadlock detection in M4.

### 1.2 Call
- `call` stores a fixed inline message in the endpoint and blocks its caller.
- If a receiver is already waiting, the IPC mechanism emits a wake effect for that receiver immediately.

### 1.3 Receive and Reply Authority
- `receive` either blocks one receiver or delivers the pending caller's message.
- Delivery creates exactly **one** reply authority tied to the endpoint, the endpoint generation, the caller identity, and a monotonically advanced reply sequence.

### 1.4 Reply
- `reply` consumes that reply authority exactly once and emits a wake effect for the caller.
- A stale, forged, reused, or endpoint-destroyed reply token fails cleanly, without waking an unrelated thread. M4 restricts replies to carrying bytes only (no capability transfers) because a robust all-or-nothing rollback for transfers inside a reply is deferred.

### 1.5 Destruction
- Endpoint destruction invalidates its active reply authority and emits a defined `EndpointClosed` wake/error effect for every blocked caller or receiver.

## 2. Notification Contract

A notification is a mechanism for events, independent of endpoint buffering. It is not an endpoint, queue, timer, or interrupt authority object.

### 2.1 State and Signalling
- A notification owns a `u64` pending-bit mask and at most **one waiting thread**.
- `signal(bits)` performs `pending |= bits`, never allocates, and emits a wake effect if a waiter exists. Multiple signals coalesce by bitwise OR.

### 2.2 Wait
- `wait` atomically consumes all nonzero pending bits. If the mask is zero, it records one waiter and emits a block effect.
- A signal observed before the waiter is installed wins; no wakeup is lost under the single-BSP execution model.

## 3. Capability-Transfer Contract

Gaxera transports opaque bytes and opaque capability handles. The IPC system never interprets a request as a path, command, service name, or application protocol.

### 3.1 Inline Transfers
- M4 supports a small fixed number of optional transferred capabilities per inline message. The ABI defines a strict maximum (not dynamically sized).
- Capability transfers are strictly all-or-nothing.

### 3.2 Prepare, Commit, Rollback
- **Call-side preparation** validates the source handle, rights narrowing, object liveness, and lineage, without mutating either capability space.
- **Receive-side commit** validates again and derives into the receiver's space.
- If any transfer cannot commit (e.g. target capacity exhaustion), the message remains pending and neither space observes a partial transfer. The receiver receives a defined error and may retry after making capacity available.

## 4. Scheduler Separation

Scheduler policy remains strictly outside IPC. IPC mechanisms return explicit `Block` or `Wake` effects. The BSP scheduler applies those effects using its own state rules, ensuring that the endpoint/notification state machines are independent of run-queue management.
