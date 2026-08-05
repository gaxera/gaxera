# ADR-0043: Executable Image Memory and Process Loading Boundary

**Status:** Accepted
**Date:** 2026-08-03
**Amends:** None

## Context

In v1.1.0, anonymous `MemoryObject` allocations were explicitly made non-executable (NX) to adhere to standard W^X (Write XOR Execute) security policies. However, to bootstrap actual user services, the system must allow for the loading of executable instructions. If the kernel blindly allows `Rights::EXECUTE` on any anonymous mapping or implements a monolithic ELF loader, it violates either the W^X invariant or the microkernel principle (by importing policy-laden format parsing into Ring 0). We need a mechanism to securely construct executable mappings without elevating generic anonymous memory into executable memory, and without burdening the kernel with ELF parsing.

## Decision

We introduce an explicit "Executable Image" classification for `MemoryObjects`, strictly segregated from standard anonymous heap memory.
1. **Explicit Classification**: A `MemoryObject` is classified at creation as either anonymous (always NX) or an executable image.
2. **Authority**: Executable Image `MemoryObjects` can only be created by an actor possessing a `Factory` capability explicitly authorized with image-creation rights.
3. **W^X Enforcement**: `MapMemory` categorically rejects W+X requests for any memory type. Image memory may be mapped as R-X or R--, but never W+X.
4. **Userspace Loader**: The kernel continues to load only the `init` payload. `init` acts as the userspace loader, parsing ELF headers, allocating Image `MemoryObjects` for text/rodata segments, allocating anonymous `MemoryObjects` for data/BSS segments, and issuing the appropriate `MapMemory` calls into the child's `AddressSpace`.

## Consequences

- **Easier**: The kernel remains free of complex ELF parsing logic, adhering to the microkernel philosophy.
- **Easier**: W^X security invariants are structurally enforced at the capability level. A compromised process cannot dynamically allocate an anonymous heap buffer, write shellcode to it, and map it as executable.
- **Harder**: The `init` supervisor requires a more complex `ProcessBuilder` implementation to coordinate multiple fallible allocations, parsing, and distinct mapping types while handling rollbacks cleanly on failure.

## Alternatives Considered

- **In-Kernel ELF Loader**: Moving `execve()` style ELF loading into the kernel. Rejected as it violates microkernel principles, forcing complex format parsing and arbitrary memory allocation into Ring 0.
- **W+X Transitions**: Allowing `init` to map a segment as RW, copy the ELF text into it, and then change the mapping to R-X (via `mprotect` style transitions). Rejected as it complicates the page table TLB invalidation path and creates a brief window of W+X vulnerability. The preferred approach allocates the image frame, populates it from the loader, and then maps it R-X directly into the child.
