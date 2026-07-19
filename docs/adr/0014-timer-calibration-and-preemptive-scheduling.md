# ADR 0014: Timer Calibration and Preemptive Scheduling

## Status
Accepted

## Context
Gaxera Milestone 5 requires the introduction of preemptive scheduling. Up to M4, threads could only yield cooperatively (via `sys_yield` or blocking IPC). For a true microkernel, threads must be preemptable to prevent CPU monopolization. To achieve this, a periodic timer must interrupt the running thread, allow the scheduler to evaluate time quantums, and perform context switches automatically.

The Local APIC timer is the standard architectural mechanism for per-CPU local timers on x86_64, but it lacks a standardized frequency. It must be calibrated against a known clock source before it can be used for deterministic timekeeping.

## Decision
We decided to implement the preemptive scheduling infrastructure with the following architectural choices:

### 1. PIT-Based APIC Calibration
We use the legacy Programmable Interval Timer (PIT) on the BSP to calibrate the Local APIC timer. The APIC timer is run for a known duration (measured via PIT channel 2) to determine the number of APIC ticks per millisecond.
* This avoids dependencies on ACPI PM timers, HPET, or CPUID leaf 0x15, keeping the hardware dependency surface minimal.
* Calibration happens exactly once during the BSP initialization phase before secondary cores are booted.

### 2. Lock-Free Preemption Path
Preemption is managed by a per-CPU `CpuLocal` structure containing the `Scheduler`.
* The `Scheduler` is accessed without locking by the timer interrupt handler since interrupts are disabled (IF=0) during the handler execution, and the data is CPU-local.
* Preemption only triggers a context switch if the interrupted code was in User Mode (CS == `USER_CODE_SELECTOR`). Kernel execution (e.g., system calls) is non-preemptible by design.

### 3. Unified Context Switching
Both cooperative yields and preemptive yields now share a single core `reschedule` function.
* The timer interrupt handler manually executes the context switch if the scheduler's quantum expires.
* The `TimerQueue` and `MonotonicClock` state are tracked inside `CpuLocal`.

### 4. Custom Assembly Stub for Timer Interrupts
We use a custom `naked` assembly stub (`timer_interrupt_entry`) rather than relying on `extern "x86-interrupt"` for the timer interrupt.
* This ensures strict control over the `swapgs` instruction and guarantees that the compiler does not insert a prologue before critical GS-base swaps.

## Consequences
* **Positive:** Gaxera now supports full preemptive multitasking for user threads, preventing infinite loop lockups.
* **Positive:** The system's interrupt-handling architecture is validated under heavy asynchronous loads.
* **Negative:** System call latency is slightly impacted by the overhead of checking for pending preemptions on exit (future work).
* **Negative:** Preemption tests require careful synchronization, as seen with the adjustments needed for `test-context-preservation`.
