# ADR-0041: Versioned Bootstrap Capability Manifest

**Status:** Accepted
**Date:** 2026-08-03
**Amends:** Supersedes ADR-0017

## Context

ADR-0017 established a fixed-slot convention for capability handoff during process bootstrap (e.g., slot `0` is the AddressSpace, slot `3` is the Factory). This implicit numbering model is fragile. It assumes a homogeneous runtime requirement for all processes, making it difficult to provision specialized drivers with varying device capabilities or endpoints. It also creates a vector for security issues if a process misinterprets a capability type or accesses a capability not explicitly meant for it, relying on silent slot mapping rather than explicitly authorized roles.

## Decision

We supersede ADR-0017's fixed slots with a versioned, read-only `BootstrapManifest`.
1. **Manifest Delivery**: The kernel maps a single, read-only, NX page containing the `BootstrapManifest` into the child's address space. The `RDI` and `RSI` registers contain the manifest's virtual address and length upon process entry.
2. **Explicit Roles**: Each capability entry explicitly identifies its `BootstrapRole` (e.g., `SelfAddressSpace`, `HeapFactory`, `InterruptObject`), the opaque handle value, `ObjectType`, and `Rights`.
3. **No Slot Guessing**: Child processes never infer meaning from numeric handle values. They parse the manifest, matching expected required or optional roles, and retrieve the handles based on role metadata.
4. **Validation**: Unknown versions, duplicate required roles, invalid handles, or malformed lengths cause deterministic rejection during the process creation transaction or child initialization.

## Consequences

- **Easier**: Provisioning heterogeneous processes (e.g., drivers needing distinct MMIO and IRQ capabilities) is structured, unambiguous, and dynamically scalable within the manifest bounds (e.g., up to 32 entries).
- **Easier**: The kernel remains agnostic to generic ELF loading while providing a secure mechanism for `init` to hand off policy-defined capabilities.
- **Harder**: The userspace runtime and `init` allocator must now fallibly parse the manifest before allocating memory or initializing standard libraries, requiring careful staging in `libgaxera`.

## Implementation Addendum

The implemented manifest wire version is currently **3**. Entries are 24
bytes and include a role-specific `metadata` word. `BootModule` entries use
that word for the exact module byte length; it never carries a physical
address. The initial process also receives a separate `ImageFactory` role
whose `IMAGE_FACTORY` right authorizes executable-image MemoryObjects. The
ordinary heap factory remains incapable of creating image-authorized memory.

## Alternatives Considered

- **String-based Global Namespace**: Passing string-based paths (like a VFS) to resolve capabilities dynamically. Rejected as it violates explicit capability delegation and introduces unnecessary parsing complexity and ambient authority into the kernel.
- **Variable-Length IPC Payload on Startup**: Using an explicit `StartProcess` IPC message to deliver handles. Rejected because it complicates the minimal execution bootstrap boundary and requires the child to have a running IPC loop before it even has a heap allocator.
