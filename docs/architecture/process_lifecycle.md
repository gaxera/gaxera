# Process Lifecycle and Teardown Architecture

> **Status:** Current / verified mechanism
> **Initiative:** v1.2.0 — Device Event and Ring-3 Runtime Completion

## 1. Process States
A Ring-3 process progresses through a strictly defined lifecycle:
- **New**: Instantiated but lacks required components.
- **Prepared**: Component assembly complete; manifest and thread are validated.
- **Runnable**: Ready to be scheduled.
- **Running**: Actively executing.
- **ExitRequested**: A self-exit or supervisor termination has been issued.
- **Exiting**: Detached from the scheduler.
- **Zombie**: User execution has stopped, deferred cleanup is queued, and the supervisor is signaled.
- **Reaped**: All process-owned components are released, and the supervisor has executed the `Reap` operation (terminal state).

## 2. Supervisor Authority
The supervisor retains the capability to query state, terminate, wait for exit notifications, and reap the process. Restarting a crashed or terminated service requires creating a completely new Process identity. The `Dead -> Runnable` resurrection of threads is strictly prohibited.

## 3. ExitProcess and Teardown Semantics
`ExitProcess` executes a bounded teardown phase that never returns to the exiting user thread. The teardown ordering is deterministic:
1. Stop scheduling.
2. Mask/unbind interrupt objects.
3. Close wait registrations.
4. Close Process-owned mappings.
5. Unmap user address space.
6. Delete Process-owned capabilities.
7. Release Thread and kernel stack.
8. Release AddressSpace page-table frames.
9. Release Process reference to ResourceDomain.
10. Signal supervisor exit Notification.
11. Mark Zombie.

## 4. Crash Semantics
If a process faults in Ring 3 (e.g., Page Fault), the kernel safely forces it to the `ExitRequested` state, stops further syscall processing from it, and initiates the deterministic teardown, ultimately notifying the supervisor via the exit Notification.

## 5. Delegated Capability Lifetime
A child process receives a dedicated `ResourceDomain` reserved from its parent. If Process A delegates a `MemoryObject` to Process B, and Process A exits, the `ResourceDomain` associated with the `MemoryObject` remains alive and correctly charged until Process B drops the capability and releases the memory.

## 6. Restart and Rebind
Restarting a driver requires creating a fresh Process, Thread, AddressSpace, and CapabilitySpace. Device capabilities and interrupt vectors are cleanly rebound because the previous Process teardown unbinds and masks the old interrupt state, bumping the vector generation. Stale capabilities or vector generations fail safely.

## 7. Verification Requirements
- `test-process-create-start-exit`: Process creation, Prepared state,
  supervisor termination, Zombie transition, and Reap. It does not yet claim
  successful executable child-image dispatch.
- `test-process-supervisor-terminate`: Supervisor forced termination.
- `test-process-reap`: Safe transition to Zombie and subsequent cleanup.
- `test-process-restart`: Validates repeated create/terminate/reap mechanics;
  it does not yet restart a running driver.
- `test-process-delegated-memory`: verifies a real cross-process child image,
  manifest-discovered delegated capability, creator exit, surviving mapping,
  second-child access, and final reclamation.
- `test-driver-crash-restart`: verifies the integrated real-driver lifecycle.
  A generated Ring-3 VirtIO RNG image receives its real bootstrap, DMA, IRQ,
  Notification, and VirtIO queue capabilities, completes an interrupt-driven
  request, exits deliberately, is reaped, and is replaced by a fresh process
  with fresh device capabilities.
