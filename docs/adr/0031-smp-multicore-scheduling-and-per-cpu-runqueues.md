# ADR 0031: SMP Multi-Core Scheduling & Per-CPU Runqueues

> **Status:** Proposed  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.9.4 — SMP Multi-Core Scheduling & Per-CPU Runqueues (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `kernel`, `kernel-core`  

---

## Context & Problem Statement

Gaxera currently runs on a single bootstrap processor (BSP). Multi-core hardware execution requires booting Application Processors (APs), maintaining independent per-CPU scheduling queues, and providing cross-CPU Inter-Processor Interrupt (IPI) notifications for thread preemption and rescheduling.

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

### 2. Deferred Preemption Model (`need_resched`)
- To prevent preemption deadlocks inside spinlocks or IPC fast paths:
  1. Cross-CPU Reschedule IPI handler sets `CpuLocal.need_resched = true` and acknowledges Local APIC EOI.
  2. Preemption triggers **only** at safe preemption boundaries:
     - Upon returning from an interrupt/exception handler to user space.
     - When `preemption_disabled_depth` decrements to 0.

### 3. Architecture-Neutral IPI Abstraction Layer
- Vector numbers (`0xFD` Reschedule, `0xFC` TLB Flush) are private to `kernel::arch::x86_64`.
- Higher-level kernel subsystems invoke architecture-neutral APIs:
  - `arch::smp::send_reschedule_ipi(cpu_id)`
  - `arch::smp::send_tlb_flush_ipi(cpu_id)`

### 4. Hard CPU Affinity & Thread Migration
- `ThreadObject` enforces hard affinity (`affinity_mask: u32`).
- Thread migration is permitted **only** when a thread is in `Runnable` state (never while `Running`).
- Migration dequeues thread from source CPU runqueue, updates `assigned_cpu`, and enqueues into target CPU runqueue under target scheduler lock.

### 5. Policy-Independent `Scheduler` Abstraction
- Per-CPU `Scheduler` instances use priority FIFO queues for Milestone 0.9.4.
- The `Scheduler` interface remains policy-independent, allowing future scheduling algorithms (EDF, CFS) to replace FIFO without modifying the `CpuLocal` architecture.

### 6. Subsystem Isolation: TLB Shootdown Separated from Scheduler
- TLB shootdowns belong **100% to the Virtual Memory subsystem** (`kernel::arch::x86_64::paging`).
- Scheduler code has zero knowledge of page tables or TLB invalidations.

---

## Non-Goals for ADR 0031

- Automatic dynamic load-balancing or work-stealing (deferred to v1.0).
- NUMA memory topology optimization.

---

## Consequences & Invariants

1. **Zero Preemption Deadlocks:** Deferred preemption via `need_resched` eliminates kernel spinlock preemption hazards.
2. **Scalable Multi-Core Architecture:** Cache-isolated `CpuLocal` structures eliminate global scheduler locks.
