# ADR 0025: Service Discovery Registry & Bootstrap Protocol

> **Status:** Accepted  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.8.4 — Service Discovery (`docs/roadmap/roadmap_v08.md`)  
> **Applies To:** `init`, `libgaxera`, `crates/`  

---

## Context & Problem Statement

In Gaxera's capability microkernel architecture, processes execute in isolated address spaces with zero ambient access to system services or hardware endpoints. To establish a functioning multi-service OS userland:

1. A service manager service (`init`) must coordinate system startup and maintain a service registry.
2. System services (`ramfs`, `console`, `script_session`) must register their names and endpoints with `init`.
3. Client processes require a standardized IPC name lookup protocol to query service endpoints by string name (e.g. `"gaxera.svc.ramfs"`, `"gaxera.svc.console"`).
4. Endpoint capabilities must be securely transferred from service providers to clients using capability delegation (`TransferDescriptor` / CSpace capability transfer).

---

## Technical Decisions

### 1. Dedicated Wire Types & Protocol Extensibility (`gaxera-abi::service`)
- **`ServiceName` Wire Layout:** Defined as `#[repr(transparent)] pub struct ServiceName([u8; 32]);`. This guarantees exact 32-byte layout, zero padding overhead, and ABI stability. Validation (ASCII/UTF-8 check, non-empty, length $\le 32$) and zero-padding are enforced via constructors (`try_from(&str)`) and accessor methods (`as_str()`).
- **`ServiceHeader`:** A compact 64-bit extensible header:
  - `version: u16` (current `1`)
  - `op: u16` (`Register = 1`, `Lookup = 2`, `Response = 3`, reserved `Unregister = 4`)
  - `status: u32` (`Success = 0`, `NotFound = 1`, `AlreadyExists = 2`, `AccessDenied = 3`, `RegistryFull = 4`, `InvalidName = 5`)
- **Lifecycle Scope:** `ServiceOp::Unregister` (opcode `4`) is reserved in the protocol for future supervision milestones; active unregistration lifecycle semantics are deferred until service supervision is implemented.

### 2. Registry Ownership & Capability Transfer Model
- **`ServiceRegistry` Ownership:** `init` permanently owns type-safe `EndpointHandle` instances wrapping `OwnedHandle` in its internal registry table.
- **Capability Delegation:** Upon successful `Lookup`, `init` derives a capability grant into the client's CSpace via `TransferDescriptor` during the IPC `Reply`. `init` retains its own original `EndpointHandle` in the registry.
- **Client Lifetime:** The requesting client receives an independent `OwnedHandle` in its CSpace; dropping it deletes only the client's handle slot without invalidating the registry or other clients.

### 3. User-Space Registration Authorization & Duplicate Policy
- **Registration Authorization:** Registration is restricted to authorized bootstrap services. Unprivileged client attempts to call `Register` are rejected by `init` with `ServiceStatus::AccessDenied`.
- **Duplicate Policy:** **Reject duplicates**. Attempting to register a name that already exists returns `ServiceStatus::AlreadyExists`.

### 4. Failure Semantics & Bounded Scalability
- **Explicit Protocol Errors:** Lookup failure (non-existent service) returns an IPC response payload with `ServiceStatus::NotFound` rather than failing the IPC transport.
- **Deterministic Scalability:** `ServiceRegistry` is bounded to `MAX_SERVICES = 32` entries. Insertion when full returns `ServiceStatus::RegistryFull` with zero dynamic allocation.

---

## Consequences & Invariants

1. **Userland Policy Separation:** All naming, authorization, and registry policies reside in `init` (ring 3), adhering strictly to ADR 0013.
2. **ABI & Layout Stability:** `#[repr(transparent)] struct ServiceName([u8; 32])` guarantees predictable 32-byte layout without hidden struct field padding.
3. **Deterministic Behavior:** Bounded memory and $O(N)$ scanning ($N \le 32$) ensure bounded latency.
4. **Capability Security:** Zero ambient handle leak; capability grants are explicitly derived and delegated per client lookup.
