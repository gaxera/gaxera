# Gaxera Microkernel Architecture — IPC Evolution & Multiplexing

> **Status:** Completed / Canonical Baseline   
> **Epoch:** Epoch 2 (v0.7)  
> **Initiative:** Initiative #2 — IPC Evolution Architecture  
> **Created:** 2026-07-23  
> **Document Type:** Living Architecture Document  

---

## 1. Program Charter

### 1.1 Problem Statement
The v0.5 IPC model established synchronous, capability-mediated rendezvous transfers between threads via `Endpoint` and `Notification` primitives. While effective for basic closed-loop payloads (such as `init` launching `ramfs` and `script_session`), empirical architectural analysis reveals three fundamental scalability barriers that prevent Gaxera from operating multi-client microkernel services:

1. **Strict 1:1 Rendezvous Restriction:** A Gaxera `Endpoint` currently holds at most *one* caller thread and *one* receiver thread. If a second caller attempts to invoke `OperationCode::Call` on a busy server endpoint, the call fails immediately with `ResourceLimitExceeded` instead of queueing or blocking safely. This prevents multi-client microkernel services (e.g. filesystem, window manager, network stack) from accepting requests from multiple client processes.
2. **Lack of Multiplexed Waiting (No $N$-Source Waiting):** A server process can currently only block on a single endpoint via `OperationCode::Receive` or a single notification via `OperationCode::Wait`. It cannot wait on multiple IPC endpoints, notifications, or timer objects simultaneously. This forces servers to spin or spawn dedicated worker threads per channel, causing severe kernel thread stack overhead and CPU idling.
3. **Absence of Priority Inheritance across IPC Chains:** When a high-priority client thread invokes a low-priority server endpoint, the server thread continues executing at its static lower priority. If an intermediate-priority thread preempts the server thread, the high-priority client is indirectly blocked, introducing unbound priority inversion.
4. **Syscall Fast-Path Latency:** IPC syscall handling currently traverses full capability table lookups and generic syscall frame dispatch routines. Without an assembly-level fast path for 64-byte inline registers (`call`/`reply` register handoff), IPC round-trip latency is higher than optimal for microkernel performance.

### 1.2 Scope
This initiative governs the architecture of microkernel Inter-Process Communication across message queueing, waiting primitives, priority mechanics, and assembly fast-paths:
* **Multi-Client Server Queueing:** Architecture for N-client to 1-server IPC queueing, caller wait queues, and capability-bounded queue depths.
* **Multiplexed Waiting Primitives:** Designing a capability-mediated multiplexing mechanism (e.g. `WaitSet`, `PollSet`, or composite notification/endpoint binding) enabling a thread to block on $N$ event sources with atomic wakeups.
* **Priority Inheritance & Priority Propagation:** Formalizing priority propagation algorithms across IPC call chains to guarantee real-time response bounds and prevent priority inversion.
* **Fast-Path IPC Assembly Transitions:** Assembly-level fast-path mechanics (`sys_call` / `sys_reply`) bypassing heavy kernel overhead when registers alone contain the message payload.
* **Kernel & User IPC Error Recovery:** Transactional state machine semantics for caller cancellation, server crash recovery, and capability transfer rollbacks under unhandled faults.

### 1.3 Non-Goals
* **Asynchronous Unbounded Messaging Buffers:** Gaxera will not implement arbitrary userspace-allocated asynchronous message queues in the kernel; zero-copy shared memory (`MemoryObject`) remains the primitive for bulk asynchronous data streams.
* **SMP Lock-Free IPC Routing:** SMP-specific cross-core IPC dispatch mechanisms belong to the future SMP Architecture initiative (Program F).
* **POSIX Socket Emulation in Kernel:** POSIX socket APIs must be implemented in userspace libraries (`libgaxera`), not inside the microkernel IPC subsystem.

### 1.4 Key Questions for Research
During the Research & Validation stage, this initiative must answer:
1. **Queueing vs. Binding Architecture:** Should multi-client support be achieved via caller queues inside `Endpoint` objects, or via userspace-managed channel arrays bound to a unified `WaitSet` kernel object (similar to seL4 notification binding or Zircon port/channel semantics)?
2. **Priority Inheritance Bound:** How should priority inheritance traverse nested IPC chains (Client A → Server B → Driver C) without lock deadlocks or unbounded priority inheritance recursion?
3. **Multiplexed Wait Semantics:** How can `WaitSet` / multiplexed waiting report which specific handle triggered the event without allocating dynamic heap memory inside the kernel trap path?
4. **Assembly Fast-Path Boundaries:** What minimal set of CPU registers must be preserved across the fast-path assembly trampoline to achieve sub-microsecond IPC round-trips while maintaining capability validation invariants?

### 1.5 Affected Subsystems
* `crates/gaxera-abi`: `OperationCode`, IPC syscall ABI, error status codes, and `WaitSet` handle types.
* `crates/kernel-core`: `ipc.rs` state machines, `Endpoint`, `Notification`, `CapabilitySystem` validation.
* `kernel/src/arch/x86_64`: `syscall.rs` assembly trampolines, register passing, fast-path context switches.
* `crates/init` & `crates/ramfs`: Multi-client server loop migration and multiplexed event handling.

### 1.6 Completion Criteria
1. **Multi-Client IPC Verification:** A single server process handles concurrent IPC requests from at least 16 client processes without packet drop or resource exhaustion.
2. **Multiplexed Wait Verification:** A server thread blocks on a single `WaitSet` containing at least 4 distinct endpoints and notifications, receiving atomic wakeups with correct event mask indicators.
3. **Priority Inheritance Proof:** Micro-benchmark proves that a high-priority caller elevates a low-priority server thread's effective priority during IPC processing, preventing preemption by medium-priority background threads.
4. **Fast-Path Performance Benchmark:** Fast-path assembly IPC round-trip latency measured and confirmed under QEMU.
5. **Zero Frame / Memory Leak Invariant:** Zero kernel heap or physical frame leaks across 10,000 multi-client call/reply and wait cycles.

---

## 2. Problem Space & Architectural Tradeoffs

### 2.1 Single-Caller vs Multi-Caller Queueing

| Model | Strengths | Weaknesses | Gaxera Fit |
| :--- | :--- | :--- | :--- |
| **Strict 1:1 Rendezvous (v0.5)** | Zero queue memory, trivial state machine, deterministic timing | Cannot serve multiple clients concurrently | **Current Baseline (Too Limiting)** |
| **Model A: Bounded Endpoint Call Queues + Badged Notifications** | Reuses existing kernel objects, zero-allocation fast-path, seL4-proven determinism | Limited when polling disparate object types (timers + endpoints + notifications) | **Candidate A (Microkernel Minimal)** |
| **Model B: First-Class `WaitSet` Kernel Objects** | Maximum flexibility (`epoll`/`zx_port` model), uniform multi-object polling | New object type (`ObjectType::WaitSet`), cross-object registration tracking on destroy | **Candidate B (OS Scalability)** |

### 2.2 Detailed Comparative Analysis of Candidates

#### Candidate Model A: Bounded Rendezvous Queue + Notification Badging
* **Mechanism:**
  - `Endpoint` maintains a bounded queue of calling threads (`CallerQueue<CallerEntry>`). Up to $N$ clients (e.g. 16 or 32) can be blocked in `CallerPending` state simultaneously.
  - `Notification` capabilities support *badging* (`Rights::DERIVE` with a 64-bit badge payload). When a badged notification is signaled, its badge is bitwise-ORed into the target notification word.
  - A thread can *bind* a `Notification` to an `Endpoint`. A call to `OperationCode::Receive` on the endpoint checks for pending badged notification signals before popping from the endpoint caller queue.
* **Kernel Memory Impact:** Bounded, fixed-size queue per `Endpoint` allocated via `SlabCache<Endpoint>`. No dynamic allocation on syscall fast path.
* **Teardown Semantics:** Clean and simple. Destroying an `Endpoint` unblocks all queued callers with `EndpointError::Closed`.

#### Candidate Model B: First-Class `WaitSet` Kernel Object
* **Mechanism:**
  - Introduce `WaitSet` as a new kernel object (`ObjectType::WaitSet = 13`).
  - A `WaitSet` holds a queue of ready event signals (`WaitEvent { cookie: u64, signals: u32 }`).
  - A server thread registers endpoints, notifications, and timer objects into a `WaitSet` via `OperationCode::WaitSetControl(waitset_handle, ADD, target_handle, user_cookie, signal_mask)`.
  - When state changes occur on a registered target object (e.g. a client calls an endpoint, or a timer fires), a `WaitEvent` is posted to the `WaitSet`.
  - A thread invokes `OperationCode::WaitSetWait(waitset_handle, timeout)` to block until ANY registered object is ready.
* **Kernel Memory Impact:** Requires a slab-allocated event queue per `WaitSet`.
* **Teardown Semantics:** Requires cross-link cleanup: when a registered object (e.g., an `Endpoint`) is destroyed, all `WaitSet` subscriptions referencing it must be invalidated.

---

## 3. Chosen Architectural Specification: First-Class `WaitSet` & Bounded Multi-Client Queueing

Following trade-off evaluation and architectural review, Gaxera adopts **Model B (First-Class `WaitSet` Kernel Objects)** as the canonical IPC architecture for Epoch 2 (v0.7).

### 3.1 Multi-Client `Endpoint` Caller Queueing
1. **Bounded FIFO Queue:** `Endpoint` replaces single `CallerPending` state with a bounded `CallerQueue` holding up to 32 caller entries (`CallerEntry { caller: ObjectId, message: InlineMessage }`).
2. **Deterministic Teardown:** Destroying an `Endpoint` unblocks all queued caller threads with `EndpointError::Closed`.

### 3.2 `WaitSet` Object Specification
1. **Object Identity:** `ObjectType::WaitSet = 13`.
2. **Registration & Subscriptions:** Server processes register target objects (`Endpoint`, `Notification`, `Timer`) into a `WaitSet` via `WaitSetControl`:
   - `ADD`: Registers an object with a `user_cookie: u64` and `signals: u32` mask.
   - `REMOVE`: Unregisters an object.
3. **Atomic Multi-Source Waiting:** `OperationCode::WaitSetWait` blocks the calling thread until an event occurs on any registered handle. On wakeup, the kernel copies an array of `WaitEvent { cookie: u64, signals: u32 }` back to user space without dynamic allocation.

### 3.3 IPC Priority Inheritance Algorithm
1. **Priority Propagation:** When Client $C$ (priority $P_C$) calls Server $S$ (priority $P_S$ where $P_S < P_C$), Server $S$'s effective priority is elevated to $P_C$.
2. **Restoration on Reply:** Upon completing `OperationCode::Reply`, $S$'s effective priority reverts to $P_S$.

---

## 4. Invariants & Security Boundaries

1. **Capability Isolation:** A thread can only wait on or receive messages from endpoints and notifications for which it holds explicit `Rights::RECEIVE` or `Rights::WAIT` capabilities.
2. **Bounded Kernel Memory:** Multiplexed waiting and caller queueing must never allocate dynamic heap memory on the fast-path.
3. **Atomic Rollback:** If a caller thread is destroyed or cancelled while queued on an endpoint, its entry is atomically removed from the wait queue without corrupting server state.

---

## 5. Interface & ABI Impact (Planned)

### 5.1 New ABI Opcodes (Tentative)
* `OperationCode::CreateWaitSet = 10`
* `OperationCode::WaitSetControl = 11` (Add/Remove handle)
* `OperationCode::WaitSetWait = 12`

---

## 6. Verification & Evidence Plan

1. **Multi-Client Stress Profile:** QEMU integration test profile `test-ipc-multiclient` spawning 16 client threads calling a single server endpoint concurrently.
2. **Multiplexed Wait Profile:** Integration test profile `test-ipc-waitset` verifying atomic multi-source wakeups.
3. **Priority Inheritance Profile:** Integration test profile `test-ipc-priority-inheritance` verifying high-priority caller boost.

---

## 7. Deferred Decisions

1. **Cross-Core SMP Fast-Path:** Deferred to SMP Architecture initiative (Program F).
2. **IPC Flow Control / Rate Limiting per Domain:** Deferred to Resource Domain evolution.
