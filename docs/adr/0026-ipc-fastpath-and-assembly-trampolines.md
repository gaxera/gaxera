# ADR 0026: IPC Fast-Path & Assembly Trampoline Optimization

> **Status:** Accepted  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.8.5 — IPC Performance & Assembly Fast Paths (`docs/roadmap/roadmap_v08.md`)  
> **Applies To:** `kernel`, `libgaxera`, `kernel-core`  

---

## Context & Problem Statement

In capability microkernel design (e.g., seL4), IPC performance directly dictates system throughput. In Gaxera:

1. High-frequency client-server transactions (e.g. `script_session` to `ramfs` or `console`) perform synchronous `Call` $\rightarrow$ `Reply` exchanges.
2. When the receiver thread is already blocked waiting on the target `Endpoint`, routing through generic scheduler queue management adds unnecessary overhead.
3. We require an optimized IPC fast-path that direct-switches execution while preserving capability security, subsystem separation, scheduler authority, and semantic equivalence.

---

## Technical Decisions

### 1. Structured Fast-Path Decision & Fallback Telemetry
- **`FastPathDecision`:** IPC eligibility evaluation returns a structured enum:
  - `FastPathDecision::Eligible { receiver_thread: ObjectId }`
  - `FastPathDecision::Rejected(FastPathRejectReason)`
- **`FastPathRejectReason` Variants:** `InvalidHandle`, `RightsDenied`, `NoReceiverWaiting`, `MultipleWaiters`, `CapabilityTransferRequested`, `PayloadTooLarge`, `FaultOrCancellation`, `SchedulerDeclined`.
- **Telemetry & Diagnostics:** Enables hit/miss telemetry and benchmark logging without re-evaluating eligibility.

### 2. Scheduler Authority & `try_direct_switch()`
- **Scheduler Ownership:** IPC evaluates capability eligibility and requests a direct switch.
- **Scheduler Approval:** The scheduler subsystem evaluates CPU pinning, affinity, and scheduler policy via `Scheduler::try_direct_switch(from_thread, to_thread) -> Result<(), FastPathRejectReason>`.
- **Transparent Fallback:** If the scheduler declines (`SchedulerDeclined`), execution transparently falls back to generic scheduler dispatch with zero side effects.

### 3. Fast-Path Eligibility Preconditions
- Fast-path execution requires **all** of the following conditions:
  1. Valid target `Endpoint` handle and matching generation.
  2. Caller holds required capability rights (`Rights::CALL` / `Rights::REPLY`).
  3. Exactly one receiver thread is currently blocked waiting on the endpoint.
  4. Message payload fits within architecture-defined inline capacity (`arch::INLINE_IPC_CAPACITY`, 64 bytes).
  5. No CSpace capability graph transfers requiring complex derivation.
  6. No thread cancellation, timeout, or pending fault state.
  7. No multi-waiter or `WaitSet` event multiplexing active.
- **Fallback Invariant:** Any rejection reason transparently falls back to baseline slow-path IPC.

### 4. Architecture Isolation & Semantic Equivalence
- **Architecture Isolation:** `kernel-core` depends only on `arch::INLINE_IPC_CAPACITY`. x86_64 register trampolines (`rdi`, `rsi`, `rdx`, `r10`, `r8`, `r9`) are encapsulated strictly in `arch/x86_64`.
- **Observational Equivalence:** Fast-path IPC produces identical observable side-effects (capability validation, reply token generation, thread state transitions) as slow-path IPC.

### 5. Correctness-First Multi-Metric Benchmark Methodology
- Every benchmark iteration asserts **exact semantic equivalence** (payload bytes, status codes, token validity) before recording timing metrics.
- The microbenchmark suite reports min, avg, p95, p99 latency cycles, throughput, and relative speedup vs. baseline slow-path IPC.

---

## Consequences & Invariants

1. **Subsystem Separation & Scheduler Authority:** IPC evaluates eligibility; scheduler retains final authority over direct context switching.
2. **Determinism:** Fast-path execution is $O(1)$ with zero dynamic memory allocation.
3. **Safety & Fallback:** Unconditional fallback to slow-path IPC on any eligibility mismatch or scheduler rejection.
