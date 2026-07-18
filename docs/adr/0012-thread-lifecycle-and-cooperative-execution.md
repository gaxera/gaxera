# ADR 0012: Thread Lifecycle and Cooperative Execution Context

**Status:** Accepted
**Date:** 2026-07-18
**Deciders:** Gaxera project

## Context

Gaxera M2 established isolated user address space mapping (`UserPageTables`), hardware `syscall`/`sysret` execution, CPU-local storage (`CpuLocal`), and fault-recoverable user memory access (`copy_from_user` / `copy_to_user`). However, execution was limited to a single static bootstrap probe stack and an single control flow without context switching or thread lifecycle management.

Before timer-driven preemption (M5) or IPC blocking (M4) can be introduced, the kernel requires a clear task lifecycle, kernel stack ownership per thread, cooperative context switching, and a single-CPU run queue abstraction. Combining context-switching with preemption or complex scheduling policies prematurely would make debugging stack, trap-frame, and register-corruption defects impossible.

## Decision

M3 establishes thread objects, kernel stack ownership, cooperative context switching, and a single-BSP run queue.

### 1. Explicit Thread Ownership & State Machine

A `Thread` is a first-class kernel object managed within `kernel-core::thread` and owned by an `ObjectArena` within a `ResourceDomain`. Each thread maintains an explicit state:

```text
[New] -> [Runnable] <-> [Running] -> [Blocked]
             |              |           |
             +------------->+---------->+---> [Dying] -> [Dead]
```

- `New`: Initialized, allocated, but not yet scheduled.
- `Runnable`: Ready for execution on the run queue.
- `Running`: Actively executing on the processor. Exactly one thread is `Running` on the bootstrap processor at any time.
- `Blocked`: Waiting on an endpoint, notification, or explicit sync condition.
- `Dying`: Execution terminated; resources pending reclamation.
- `Dead`: Completely reaped and invalidated.

#### Ownership and Lifetime Contracts
- **Kernel Stack**: A `Thread` owns its dedicated kernel stack. The stack allocation lifetime is strictly bound to the `Thread` object's lifecycle.
- **Address Space**: A `Thread` holds a reference to its `AddressSpace` (the underlying `UserPageTables`). The address space is independently reference-counted and outlives the thread if other threads or objects reference it.
- **Resource Destruction**: A thread transitioning to `Dying` cannot free its own stack while executing on it. The scheduler (or a dedicated reaper thread) reaps `Dead` threads, safely releasing the stack memory and returning the `Thread` object slot to its arena.

### 2. Scheduler Ownership (Per-CPU Evolution)

The single-BSP run queue is implemented as a `Scheduler` instance owned by `CpuLocal`, rather than a global static lock. This guarantees that run queue manipulation is bound to the executing processor and naturally aligns with future SMP evolution, avoiding architectural redesign when APs are introduced.

### 3. Context-Switch Invariants (`arch/x86_64/context.rs`)

- Each thread owns a dedicated, page-aligned kernel stack (16 KiB default) allocated from kernel memory, bounded below by an unmapped guard page.
- `TSS.RSP0` and `CpuLocal.kernel_stack_top` are updated to point to the top of the current thread's kernel stack upon every context switch.
- No thread may free or deallocate its kernel stack while actively executing on that stack. Dead threads are reaped asynchronously or by another executing thread.

### 3. Cooperative Context Switching Assembly (`arch/x86_64/context.rs`)

Context switching between two kernel stacks is performed cooperatively via assembly:

```rust
pub struct Context {
    pub rsp: u64,
}
```

The switch function `context_switch(prev: *mut Context, next: *const Context)` enforces the following architectural invariants:
- **Register Preservation**: Saves callee-saved registers (`%rbx`, `%rbp`, `%r12`, `%r13`, `%r14`, `%r15`) and the return instruction pointer (`%rip`) onto the current stack, updates `prev.rsp`, loads `next.rsp`, restores the next thread's callee-saved registers, and returns into the next thread's continuation.
- **Stack Alignment**: RSP must remain 16-byte aligned before any `call` instruction or returning to CPL 3.
- **TSS & CpuLocal Updates**: `TSS.RSP0` and `CpuLocal.kernel_stack_top` are atomically updated to point to the incoming thread's stack *before* interrupts or traps are re-enabled.
- **CR3 Switching Rules**: CR3 is only reloaded if the target thread's `AddressSpace` physical root differs from the currently active CR3, avoiding unnecessary TLB flushes.
- **Lock-Free Boundary**: Memory barriers and stack pointer validity checks ensure no spinlock or interrupt-disabling lock is held across a context switch boundary.

### 4. Integration with Traps and `Syscall::Yield`

- If `next` belongs to a different `UserPageTables` address space root, `context_switch` activates the new address space root (`CR3`).
- The transient `SyscallFrame` generated during a system call lives on the calling thread's own kernel stack. When a thread calls `Syscall::Yield`, its state becomes `Runnable`, it is placed on the run queue, and `context_switch` switches to the next thread's kernel stack.

### 5. Cooperative Yielding (`Syscall::Yield`)

- M3 exposes a versioned `Syscall::Yield` system call.
- Calling `yield` suspends the current thread, selects the next runnable thread from the single-CPU FIFO run queue, and performs a context switch.
- If no other thread is runnable, `yield` returns immediately to the caller without unnecessary stack or CR3 mutation.

### 6. Extended State Deferral

User-mode extended state (FPU/SSE/AVX registers via `fxsave`/`xsave`) remains prohibited in user probe binaries until M3 context switching explicitly incorporates save/restore areas.

## Consequences

- Kernel context switching becomes deterministic, testable on the host, and proven under QEMU before preemption or IPC complexity exists.
- The transient single M2A transition stack is replaced by dynamic, per-thread kernel stack management.
- Preemption, timer interrupts, EEVDF, priority inheritance, and multi-core scheduling remain strictly deferred.
- Context preservation is formally verified by a dedicated test asserting callee-saved registers survive across multiple cooperative yields.

## Alternatives Considered

- **Immediate APIC Preemption:** Rejected because context-switching correctness must be proven cooperatively before introducing asynchronous interrupt races.
- **EEVDF / Complex Scheduler:** Rejected as premature; scheduling policy requires stable thread lifecycles and timekeeping abstractions first.
- **Shared Kernel Stack for All Threads:** Rejected because nested traps, user copies, and blocking syscalls require independent stack execution state per thread.
