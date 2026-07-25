# ADR 0031: SMP Multi-Core Scheduling & Per-CPU Runqueues

> **Status:** Approved (Clarified for Milestone 0.9.2)  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.2 — SMP Load Balancing & Inter-Core Scheduling (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `kernel`, `kernel-core`  

---

## Context & Problem Statement

Gaxera multi-core hardware execution requires booting Application Processors (APs), maintaining independent per-CPU scheduling queues, providing cross-CPU Inter-Processor Interrupt (IPI) notifications, and executing dynamic work stealing and thread migration across up to 64 CPUs while preserving strict lock ordering (ADR 0033).

---

## Architectural Invariants

> **Invariant 1 — Thread Exclusivity:**  
> A thread can never simultaneously exist on multiple CPU runqueues or be scheduled on multiple CPU cores. A thread is strictly owned by exactly one CPU runqueue at any point in time.
> 
> **Invariant 2 — Deferred Preemption Safety:**  
> Preemption is never executed directly inside IPI handlers or spinlock critical sections. Reschedule IPIs set a per-CPU `need_resched` flag, deferring preemption to safe kernel boundaries (user-space interrupt exit or preemption depth return).
> 
> **Invariant 3 — Per-CPU Cache Isolation:**  
> Each `CpuLocal` structure is 64-byte cache-line aligned (`#[repr(C, align(64))]`), guaranteeing zero false sharing or cache-line ping-pong between CPU cores during local scheduling.
> 
> **Invariant 4 — Transactional Thread Migration:**  
> Thread migration across CPUs must satisfy four non-negotiable rules:
> 1. A thread must never be runnable on two CPUs simultaneously.
> 2. Migration must occur under required scheduler/lock guards.
> 3. Capability state, `AddressSpaceToken`, and CSpace ownership must be 100% preserved.
> 4. `assigned_cpu` must be updated as part of the migration transaction.

---

## Technical Decisions

### 1. `CpuLocal` Structure & GS Register Locality
- Per-CPU state is owned by `CpuLocal` (stored in `IA32_GS_BASE`):
  ```rust
  #[repr(C, align(64))]
  pub struct CpuLocal {
      pub cpu_id: u32,
      pub lapic_id: u32,
      pub kernel_stack_top: u64,
      pub current_address_space: u64,
      pub preemption_disabled_depth: u32,
      pub interrupt_disabled_depth: u32,
      pub need_resched: bool,
      pub scheduler: Scheduler,
  }
  ```

### 2. Policy-Independent `Scheduler` Primitives vs. `SchedulerDomain` Topology
- The `Scheduler` component provides pure mechanics (`enqueue`, `dequeue_next`, `pop_stealable_work`). It contains zero policy decisions about which CPU to balance.
- Top-level topology and load balancing policy are encapsulated by `SchedulerDomain` (which owns `LoadBalancer` policy), supporting future NUMA, CPU cluster, and SMT/heterogeneous core extensions.

### 3. Centralized IPI Vector Constants
- Vector numbers are defined as centralized named constants:
  - `pub const IPI_VECTOR_RESCHEDULE: u8 = 0xFD;`
  - `pub const IPI_VECTOR_TLB_FLUSH: u8 = 0xFC;`
- Higher-level kernel subsystems invoke architecture-neutral APIs:
  - `arch::smp::send_reschedule_ipi(cpu_id)`
  - `arch::smp::send_tlb_flush_ipi(cpu_id)`

### 4. `CpuAffinityMask` Abstraction
- Raw bitmasks are replaced by `CpuAffinityMask`, hiding bitwise representation and allowing transparent scaling beyond 64 CPUs without changing public APIs.

### 5. Deterministic Thread Migration Protocol & Lock Synchronization (ADR 0033)
- Migration and work stealing between CPU `A` and CPU `B` acquire per-CPU scheduler locks in lower-to-higher CPU ID order (`min(cpu_A, cpu_B)` first, then `max(cpu_A, cpu_B)`), adhering strictly to ADR 0033 lock rank ordering to eliminate cross-CPU lock inversion deadlocks.
- The 6-step migration sequence is strictly enforced:
  1. Acquire lower-ranked scheduler lock (`min(src_cpu, dst_cpu)`), then higher-ranked scheduler lock (`max(src_cpu, dst_cpu)`).
  2. Remove target thread from source runqueue.
  3. Update thread CPU assignment (`assigned_cpu = dst_cpu`).
  4. Insert thread into destination runqueue.
  5. Trigger `send_reschedule_ipi(dst_cpu)` if destination CPU is idle or target thread outranks running thread.
  6. Release scheduler locks in reverse order.

---

## Consequences & Invariants

1. **Zero Preemption Deadlocks:** Deferred preemption via `need_resched` eliminates kernel spinlock preemption hazards.
2. **Scalable Multi-Core Architecture:** Cache-isolated `CpuLocal` structures and decoupled `LoadBalancer` policy enable predictable multi-core execution.
