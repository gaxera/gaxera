# Process Bootstrap Architecture

> **Status:** Current
> **Initiative:** v1.2.0 — Device Event and Ring-3 Runtime Completion

## 1. Program Charter
This document defines the capability handoff, resource provisioning, and structural boundaries required to bootstrap a new Ring-3 process in Gaxera.

## 2. Problem Statement
Prior to v1.2, production services relied on a dummy allocator and assumed capability handles were present at hardcoded integer slot numbers (ADR 0017). This created implicit discovery and allowed services to operate without explicit memory quotas or explicit authority delegation.

## 3. Explicit Non-Goals
- We are not building a POSIX `fork`/`exec` model.
- We are not implementing implicit dynamic capability discovery via a global namespace; all authority must be explicitly delegated.
- The kernel is not a generic ELF loader. The kernel loads only `init`. `init` remains the userspace supervisor responsible for parsing later ELF images.

## 4. Versioned Bootstrap Manifest
ADR 0017’s fixed capability slots are superseded by a versioned bootstrap manifest.

The kernel constructs a bounded, versioned, read-only `BootstrapManifest` in the child address space. The child receives its manifest pointer and length in registers:
- `RDI`: manifest virtual address
- `RSI`: manifest byte length

Each capability entry explicitly identifies:
- **Role**: e.g., SelfAddressSpace, SelfThread, HeapFactory, SupervisorEndpoint, BootModule, etc.
- **Opaque Handle**: The local handle index.
- **ObjectType**: The expected type.
- **Rights**: The explicit rights granted.

The child never infers capability roles from raw handle values.

## 5. Supervisor Responsibilities
`init` (or another authorized supervisor) is responsible for:
- Creating child processes.
- Allocating child image MemoryObjects.
- Mapping executable and writable segments.
- Installing startup capabilities into the child `CapabilitySpace`.
- Configuring the child Thread (instruction pointer, stack pointer).
- Starting the process.

## 6. Executable Image Classification
Executable memory must be explicitly classified. Anonymous memory remains NX. Executable Image MemoryObjects are created only through explicit image authority and are mapped R-X or R--, but never W+X. The loader (`init`) allocates and maps these segments.

## 7. Verification Expectations
- `test-bootstrap-manifest`: Validates the structure and layout of the manifest.
- `test-bootstrap-no-slot-assumptions`: Verifies that processes do not rely on
  fixed slot numbers.
- The manifest-backed process and IRQ profiles additionally verify that
  capabilities absent from the manifest cannot be used; this is covered by
  `test-irq-unauthorized` and the process delegation profiles.
