# Process Lifecycle and Teardown Architecture

> **Status:** Draft
> **Initiative:** v1.2.0 — Device Event and Ring-3 Runtime Completion

## 1. Process States
Define the lifecycle states of a Ring-3 process: Created, Runnable, Blocked, Exiting, Terminated.

## 2. Ownership
Who retains the capability to modify or kill a process? (Supervisor / Creator).

## 3. ExitProcess Semantics
What happens when `ExitProcess` is called? The kernel must execute a bounded teardown phase.

## 4. Crash Semantics
How does the kernel handle an unhandled CPU exception (e.g., Page Fault) in Ring 3? The crash path must safely transition the process to the Exiting state and notify its supervisor.

## 5. Resource Cleanup
- `CapabilitySpace` cleanup: dropping handles.
- `AddressSpace` cleanup: unmapping VMAs.
- `MemoryObject` cleanup: releasing physical frames when reference counts hit 0.
- `Thread` cleanup: removing from scheduler runqueues.

## 6. Delegated Capability Lifetime
If Process A delegates a `MemoryObject` to Process B, and Process A exits, the memory remains alive for Process B. How is the underlying `ResourceDomain` preserved?

## 7. Supervisor Revocation
How a supervisor process forcefully tears down a subordinate process and its delegated capabilities (via 3-tier cascade revocation).

## 8. Restart/Rebind Questions
When a driver process crashes and is restarted, how are its device capabilities and interrupt vectors rebound safely without leakage?

## 9. Failure Reporting
How is the exit code or crash dump metadata passed back to the supervisor?

## 10. Verification Requirements
- `test-clean-exit`: Process allocates memory, exits, and all frames are verified as freed.
- `test-crash-containment`: Process causes a page fault, kernel safely tears it down without panic.
- `test-delegated-survival`: Process A delegates to B, A exits, B can still access memory.
