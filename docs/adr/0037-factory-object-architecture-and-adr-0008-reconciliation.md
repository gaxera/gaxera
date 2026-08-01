# ADR 0037: Factory Object Architecture and ADR 0008 Reconciliation

**Status:** Accepted  
**Date:** 2026-07-30  
**Deciders:** Gaxera Core Maintainers  
**Supersedes:** ADR 0008 (Section 2, Paragraph 2 regarding Factory object representation)  

## Context

ADR 0008 originally specified `ResourceDomain` as the eleventh kernel object and stated: *"A Factory is not a twelfth object. It is a capability right on a ResourceDomain that permits a bounded set of creation operations."*

During the implementation of Milestone 0.8 and Milestone 0.9 (`v0.8.0` / `v0.9.0`), decoupling allocation authority from resource accounting proved necessary. Representing a Factory as an inline handle right on a `ResourceDomain` conflated the management of accounting quotas with the delegation of creation authority. To resolve this, the codebase implemented `ObjectType::Factory = 12` as a distinct kernel object stored in `OBJECT_ARENA` and the `FACTORIES` registry.

However, the implementation left a formal specification gap: current kernel syscall logic (`factory_create` in `kernel/src/arch/x86_64/syscall.rs`) checks `Rights::FACTORY` on the presented handle, but does not enforce `factory.allows(obj_type)`. Furthermore, the contradiction between ADR 0008's text and the live codebase introduces architectural ambiguity.

## Decision

1. **Reconciliation with ADR 0008:**
   ADR 0008 is formally amended and superseded regarding Factory representation. `ObjectType::Factory = 12` is accepted as a first-class kernel object in Gaxera's object taxonomy.

2. **Factory Object Representation:**
   A `Factory` kernel object is an unforgeable creation ticket that encapsulates:
   * `target_domain: ResourceDomainId`: The specific `ResourceDomain` charged for objects created via this Factory.
   * `allowed_types: ObjectTypeSet`: A bitfield mask specifying exactly which `ObjectType` kinds (e.g. `ObjectType::MemoryObject`, `ObjectType::Endpoint`) this Factory is authorized to manufacture.

3. **Capability Handle Rights:**
   `Rights::FACTORY` is a **capability handle right** stored on capability slots in `CapabilitySpace`, not an object field inside the `Factory` struct. Presenting a handle to invoke a `Factory` requires `Rights::FACTORY` on the handle slot.

4. **Creation Enforcement & Syscall ABI:**
   Object creation is invoked via `OperationCode::Call` (ABI opcode 2) suboperation 0 (`rsi == 0`) on a `Factory` capability handle. The kernel enforces:
   * The handle references a valid `ObjectType::Factory` object.
   * The handle possesses `Rights::FACTORY`.
   * The `Factory` object's `allowed_types` mask explicitly authorizes the requested `ObjectType` (`factory.allows(obj_type)`).
   * The target `ResourceDomain` has sufficient quota capacity.

   If any condition fails, the kernel aborts object creation and returns `SyscallError::RightsDenied` or `SyscallError::InvalidArgument`.

## Consequences

* **Architectural Consistency:** Resolves the historical disagreement between ADR 0008 and the repository codebase.
* **Fine-Grained Authority Delegation:** A supervisor process can manufacture and delegate a `Factory` capability restricted strictly to `ObjectType::MemoryObject` without granting authority to create `Thread`, `AddressSpace`, or `InterruptObject` capabilities.
* **Accounting Decoupling:** Allocation authority (`Factory` object) and resource quota source (`ResourceDomain`) remain cleanly separated. A process may hold a `Factory` pointing to a supervisor's `ResourceDomain` without owning or mutating the supervisor's domain.

## Alternatives Considered

**A. Preserving ADR 0008's Capability-Right-Only Model:**  
Rejected. Embedding object-creation masks directly into `CapabilitySpace` handle slots inflates capability descriptor size and complicates multi-process delegation.

**B. Unmediated Ambient Creation Syscalls (e.g. `CreateMemoryObject` without Factory):**  
Rejected. Ambient object creation violates Gaxera's core capability invariant (INV-SEC-01: no ambient authority). All object creation must be explicitly mediated by a capability handle.
