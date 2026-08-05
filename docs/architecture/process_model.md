# Process Model Architecture

> **Status:** Current / mechanism partial
> **Initiative:** v1.2.0 — Device Event and Ring-3 Runtime Completion

## 1. Program Charter
This document defines the `Process` object in Gaxera. It establishes `Process` as the primary lifecycle and containment aggregate for Ring-3 execution, explicitly describing its ownership of sub-components, supervisor relationship, and overall structural boundaries.

## 2. Problem Statement
Prior to v1.2, Gaxera defined `Thread`, `AddressSpace`, `CapabilitySpace`, and `ResourceDomain` objects, but no single entity bound them together into a managed lifecycle. This ambiguity made driver crash containment, process teardown, resource reclamation, and supervisor authority implicit rather than formal.

## 3. Explicit Non-Goals
- We are not building a POSIX `fork`/`exec` model.
- We are not implementing implicit dynamic capability discovery via a global namespace; all authority must be explicitly delegated.
- The `Process` object does not replace the responsibilities of its sub-components (e.g., `Thread` still owns execution, `AddressSpace` still owns page tables).
- The kernel does not act as a generic ELF loader or service policy engine; `init` retains that role.

## 4. The Process Object (`ObjectType::Process = 15`)
The `Process` object is a first-class kernel object that acts as a lifecycle aggregate. It coordinates startup, supervision, and teardown, but delegates the actual work to domain-specific objects.

The `Process` object explicitly owns:
- **Child `AddressSpace`**: The mapping context for the process.
- **Child `CapabilitySpace`**: The local namespace of capability handles.
- **Main `Thread` (and auxiliary Threads)**: The execution contexts belonging to the process.
- **`ResourceDomain`**: The hierarchical quota and accounting domain.
- **Process State**: (New, Prepared, Runnable, Running, ExitRequested, Exiting, Zombie, Reaped).
- **Supervisor Reference**: The capability endpoint of the process supervisor.
- **Exit Notification**: The signal delivered to the supervisor upon process termination.
- **Bootstrap Manifest**: The read-only page containing the initial capability provisioning.

The `Process` object does **not** own:
- Physical frames (owned by `MemoryObject`).
- Page table frames directly (owned by `AddressSpace`).

## 5. Supervisor Authority
The kernel provides only mechanisms. The userspace supervisor (e.g., `init` or a dedicated `DriverSupervisor`) is responsible for:
- Allocating process components (Thread, AddressSpace, ResourceDomain).
- Parsing ELF images and allocating executable/writable memory.
- Provisioning the capability manifest.
- Starting the process.
- Reacting to exit or crash notifications.
- Reaping the process when it reaches the Zombie state.
- Deciding restart and crash containment policies.

## 6. Process State Machine
A process undergoes explicit, unidirectional state transitions:
- `New -> Prepared`: Component assembly is complete.
- `Prepared -> Runnable`: Manifest and Thread are validated; process is ready to be scheduled.
- `Runnable -> Running`: Thread is actively scheduled.
- `Running -> ExitRequested`: A self-exit or supervisor termination has been issued.
- `ExitRequested -> Exiting`: The process is detached from the scheduler.
- `Exiting -> Zombie`: User execution stops, deferred cleanup is queued, and the supervisor is signaled.
- `Zombie -> Reaped`: All process-owned components have been released and the supervisor has executed the `Reap` operation. (Terminal state).
- **No `Dead -> Runnable`**: A terminated process or thread cannot be resurrected. A crashed service must be restarted from a completely fresh Process identity.

## 7. ResourceDomain Hierarchy
A `ResourceDomain` tracks limits for objects, capabilities, and physical memory. Each child process receives a dedicated `ResourceDomain` whose quota is reserved from its parent.
- Usage is charged to the child domain and accounted against the parent.
- The domain persists after process exit if delegated capabilities or MemoryObjects still reference it.
- A domain is destroyed only when all active references (process, object, capability, memory, child domains) reach zero.

## 8. Failure Rollback
Process creation (`CreateProcess`) is a transactional operation. If any internal allocation or provisioning step fails (e.g., quota exhaustion, arena exhaustion), the entire operation rolls back. No partial Process, child domain reservation, capability nodes, or page-table frames leak.

## 9. Verification Expectations
- `test-process-create-start-exit`: Validates real Process creation, Prepared
  state, supervisor termination, Zombie transition, and Reap. Executable
  child-image Start is not yet claimed by this profile; it remains dependent
  on the scheduler/context handoff proof.
- `test-process-rollback`: Validates transactional failure behavior during process creation.
- `test-process-reap`: Validates that a process transitions to Zombie and can be properly reaped.
- `test-no-resurrection`: Validates that dead threads cannot be restarted.
