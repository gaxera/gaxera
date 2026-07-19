# ADR 0017: Boot ABI and Capability Graph

## Status
Accepted

## Context
When the kernel jumps to the userspace `init` process, it must provide `init` with the necessary authority (capabilities) and information to bootstrap the system. Early designs considered passing information in multiple registers or relying on synthetic in-kernel tests to set up the environment. For a true userspace handoff, a stable Application Binary Interface (ABI) is required between the kernel and `init`.

## Decision
We establish a standardized Boot ABI for the `init` process:
1.  **BootInfo Structure:** The kernel will pass a pointer to a `BootInfo` structure in the `RDI` register. This structure contains a magic number, ABI version, and reserved fields for future expansion.
2.  **Explicit Capability Graph:** The `init` process's Capability Space (CSpace) is pre-populated by the kernel with a strict set of foundational capabilities at fixed indices:
    -   `0`: Null capability
    -   `1`: Self Thread
    -   `2`: Root Factory (Authority to instantiate objects)
3.  **Boot Modules:** Boot modules provided by Limine are tracked by name in the `BootContext` using immutable references rather than being immediately converted into complex `MemoryObject` capabilities during M6.

## Consequences
-   **Positive:** 
    -   Passing a `BootInfo` pointer instead of a magic value in a register allows future expansion (e.g., passing CPU topology or feature flags) without breaking the ABI or rewriting assembly.
    -   Pre-populating a fixed capability graph guarantees `init` has exactly the authority it needs (and nothing more), setting a strong foundation for the Principle of Least Privilege.
    -   Tracking modules by name rather than index fixes a brittle dependency on bootloader configuration ordering.
-   **Negative:** 
    -   `init` cannot currently map new memory or process complex capabilities until the generic `MemoryObject` and `AddressSpace` capabilities are implemented in M7.
