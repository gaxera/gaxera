# SMP Multi-Core Scheduling Architecture (`smp_scheduling`)

> **Status:** Current  
> **Date:** 2026-07-23  
> **Initiative:** Initiative v0.9 (Milestone 0.9.4)  
> **Applies To:** `kernel`, `crates/kernel-core`  

---

## 1. Executive Summary & Scope

The SMP Multi-Core Scheduling Architecture provides AP core bringup via ACPI MADT parsing and LAPIC INIT-SIPI-SIPI sequences, per-CPU cache-isolated state (`CpuLocal`), per-CPU `Scheduler` runqueues, deferred preemption (`need_resched`), and architecture-neutral IPI dispatching.

### Key Architectural Invariants
1. **Thread Exclusivity Invariant:** A thread can never simultaneously exist on multiple CPU runqueues or be scheduled on multiple CPU cores. A thread is strictly owned by exactly one CPU runqueue at any point in time.
2. **Deferred Preemption Safety:** Reschedule IPIs set `need_resched = true` in the target CPU's `CpuLocal` struct. Preemption executes only at safe kernel preemption boundaries (interrupt exit to user space or preemption depth return), eliminating spinlock deadlocks.
3. **Per-CPU Cache Line Isolation:** `CpuLocal` structures are 64-byte cache-line aligned (`#[repr(C, align(64))]`), preventing false sharing between CPU cores during local scheduling.

---

## 2. Component Design

### 2.1 ACPI MADT AP Core Discovery & LAPIC Bringup
- Kernel parses ACPI MADT Type 0 / Type 9 structures to locate secondary AP core Local APIC IDs.
- Executes INIT-SIPI-SIPI sequence booting AP cores into 64-bit Long Mode.

### 2.2 `CpuLocal` & GS Register Locality
- Stored in `IA32_GS_BASE`:
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

### 2.3 Architecture-Neutral IPI Abstraction Layer
- Vector numbers (`0xFD` Reschedule, `0xFC` TLB Flush) are private to `kernel::arch::x86_64`.
- Higher-level kernel subsystems invoke architecture-neutral APIs:
  - `arch::smp::send_reschedule_ipi(cpu_id)`
  - `arch::smp::send_tlb_flush_ipi(cpu_id)`

---

## 3. Verification

- QEMU integration test `test-smp-schedule` verifies AP core discovery, `CpuLocal` alignment, per-CPU runqueue scheduling, deferred preemption via `need_resched`, and cross-CPU reschedule IPI delivery.
