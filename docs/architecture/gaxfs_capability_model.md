# GaxFS Capability Model Specification

> **Status:** Authoritative Architecture Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `libgaxera::vfs`, `query_planner`  

---

## 1. Architectural Separation: OS Capability System vs. Filesystem Rights

GaxFS distinguishes between the OS-level Capability System and Filesystem-Specific Rights:
- **OS Capability System:** Manages object ownership, capability spaces (`CapabilitySpace`), handle delegation, and kernel revocation. GaxFS consumes the operating system's capability model rather than redefining it.
- **Filesystem Rights (`GaxFsRights`):** Defines filesystem-specific authorization flags attached to capability handles.

### 1.1 `CapabilityHandle` Architectural Abstraction
`CapabilityHandle` is the canonical architectural abstraction representing a delegated capability reference. Its internal encoding and kernel representations are implementation-independent; applications depend strictly on the `CapabilityHandle` interface abstraction.

---

## 2. Extensible & Orthogonal Capability Rights (`GaxFsRights`)

Filesystem rights compose orthogonally without hidden permission inheritance:

```rust
pub struct GaxFsRights;

impl GaxFsRights {
    pub const READ: u32             = 1 << 0; // Read object payload extents
    pub const WRITE: u32            = 1 << 1; // Overwrite payload extents
    pub const APPEND: u32           = 1 << 2; // Append payload data
    pub const EXECUTE: u32          = 1 << 3; // Execute payload
    pub const ENUMERATE: u32        = 1 << 4; // List namespace provider entries
    pub const SNAPSHOT: u32         = 1 << 5; // Create point-in-time snapshot
    pub const SHARE: u32            = 1 << 6; // Derive & delegate capability handles
    pub const DELETE: u32           = 1 << 7; // Unlink / logically delete object
    pub const MODIFY_METADATA: u32  = 1 << 8; // Edit metadata attributes & relationships
    pub const CREATE_CHILDREN: u32  = 1 << 9; // Create child entries in namespace
}
```

---

## 3. Core Architectural Invariants

> **Invariant 1 — Capability Derivation Monotonicity:**  
> Derived capabilities must **never** grant authority greater than the originating capability. Capability derivation may only preserve or attenuate authority. Authority amplification is impossible.
> 
> **Invariant 2 — Capability Scope Enforcement:**  
> Every filesystem operation (`resolve`, `enumerate`, `query_execute`, `modify`, `snapshot`) MUST execute relative to an explicitly presented `CapabilityHandle` scope. Zero ambient authority exists.
> 
> **Invariant 3 — Separation of Authority & Storage Lifetimes:**  
> Capability lifetime is independent of object storage lifetime. Objects may remain valid in storage after capabilities are revoked; capabilities may become invalid while objects continue to exist.

---

## 4. Observable Revocation Semantics

When a capability handle is revoked:
1. **Immediate Invalidation:** Revoked handles cannot authorize future filesystem operations.
2. **Cascading Hierarchy:** All derived capabilities in child task capability spaces lose authority according to the capability derivation tree.
3. **Future Authorization Failure:** Any future request presenting a revoked handle fails authorization immediately with `VfsError::CapabilityRevoked`.

---

## 5. Security Auditing via Event Streams

All capability lifecycle events (creation, delegation, rights attenuation, and revocation) emit `GaxFsEventType::CapabilityChanged` records to `GaxFsEventLog`. This provides:
- Authoritative security audit logs.
- Event-driven security analysis by Ring-3 `PolicyIndexProvider`.
- Historical authorization auditing without coupling capability management to application services.
