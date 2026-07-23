# Gaxera v0.7 Epoch Roadmap: Multi-Client IPC & Event Multiplexing

> **Status:** Completed  
> **Baseline:** Gaxera v0.6, tag `v0.6.0`  
> **Target:** Gaxera v0.7.0  
> **Primary Initiative:** IPC Evolution Architecture (`docs/architecture/ipc.md`)  
> **ADR Reference:** ADR 0021  

---

## 1. Executive Direction

v0.7 establishes the multi-client IPC infrastructure required for Gaxera to run concurrent microkernel services.

Where v0.6 turned memory management into a production-grade, non-leaking foundation, v0.7 expands IPC from 1:1 rendezvous into a high-performance $N:1$ server architecture with atomic multiplexed event waiting (`WaitSet`) and priority inheritance.

### Key Objectives
1. **Multi-Client Endpoint Caller Queuing:** Expand `Endpoint` to support up to 32 queued caller threads without fast-path heap allocations (`ADR 0021`).
2. **First-Class `WaitSet` Kernel Object:** Implement `ObjectType::WaitSet` (`Opcode 10`, `11`, `12`) enabling threads to wait atomically on multiple endpoints, notifications, and timers (`ADR 0021`).
3. **IPC Priority Inheritance:** Dynamically elevate server thread priority to match high-priority callers during active IPC rendezvous.
4. **Assembly Fast-Path Trampoline:** Optimize 64-byte inline register transfers for sub-microsecond IPC round-trips.

---

## 2. Milestone Structure & Acceptance Criteria

### Milestone 0.7.1: Multi-Client Endpoint Call Queueing & Bounded Caller Waiting
* **Architecture Reference:** `docs/architecture/ipc.md#31`, `ADR 0021`
* **Status:** `Complete`
* **Deliverables:**
  * Update `Endpoint` state machine in `crates/kernel-core/src/ipc.rs` with `CallerQueue` (up to 32 queued callers).
  * Implement caller queue teardown and error handling (`EndpointError::Closed`, `ResourceLimitExceeded`).
* **Acceptance Criterion:** Unit tests in `kernel-core` and QEMU integration test `test-ipc-multiclient` verify 16 concurrent client threads calling a single server endpoint without frame leaks or call rejections. (PASSED)

### Milestone 0.7.2: First-Class `WaitSet` Kernel Object & Event Multiplexing
* **Architecture Reference:** `docs/architecture/ipc.md#32`, `ADR 0021`
* **Status:** `Complete`
* **Deliverables:**
  * Add `ObjectType::WaitSet = 13` to `gaxera-abi` and `kernel-core`.
  * Wire syscall handlers for `CreateWaitSet` (10), `WaitSetControl` (11), and `WaitSetWait` (12) in `kernel/src/arch/x86_64/syscall.rs`.
* **Acceptance Criterion:** QEMU integration test `test-ipc-waitset` verifies a server thread blocking on a single `WaitSet` receiving atomic wakeups across multiple distinct endpoint and notification targets. (PASSED)

### Milestone 0.7.3: IPC Priority Inheritance & Fast-Path Optimization
* **Architecture Reference:** `docs/architecture/ipc.md#33`, `ADR 0021`
* **Status:** `Complete`
* **Deliverables:**
  * Implement `effective_priority` tracking in `thread.rs` and `scheduler.rs` for IPC caller-receiver priority boost.
  * Optimize x86_64 assembly syscall trampoline for 64-byte inline message register handoff.
* **Acceptance Criterion:** Micro-benchmark and unit tests confirm high-priority caller boost elevates server priority during IPC rendezvous and restores base priority on reply. (PASSED)

---

## 3. Explicit Non-Goals for v0.7
* Cross-core SMP fast-path IPC routing (deferred to v0.8+ SMP initiative).
* POSIX socket emulation layers inside kernel.
* Unbounded asynchronous kernel buffer queues.
