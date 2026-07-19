# ADR 0015: Message-Driven sys_invoke

## Status
Accepted

## Context
As Gaxera transitions from an initial execution environment into a genuine capability-based microkernel in Milestone 6, we must establish the system call boundary. Traditional monolithic kernels provide hundreds of distinct system calls (`sys_read`, `sys_write`, `sys_spawn`). Early microkernels provided dozens of specialized IPC and object-creation endpoints. We need to decide how userspace programs request kernel services and instantiate objects.

## Decision
We will expose exactly one primary system call for exercising authority: `sys_invoke`.
The kernel will use a message-driven capability invocation model. The syscall will accept:
1.  `cap_index`: An index into the caller's Capability Space (CSpace).
2.  `arg1..arg4`: Registers forming the invocation message.

The kernel will inspect the capability at `cap_index` and route the message to the corresponding object. If the capability is a `Factory`, the message payload will dictate what object to create (e.g., Thread, AddressSpace, Endpoint). 

## Consequences
-   **Positive:** 
    -   Absolute zero kernel policy regarding process models or file systems.
    -   ABI stability: New kernel objects do not require new syscalls; they only require defining new message structures.
    -   Capability Purity: All authority is verified simply by possessing a valid index in the CSpace.
-   **Negative:** 
    -   Slight serialization overhead compared to dedicated syscalls, as arguments must be packed into a standardized message layout.
