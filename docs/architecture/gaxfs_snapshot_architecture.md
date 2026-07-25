# GaxFS Snapshot Architecture Specification

> **Status:** Authoritative Architecture Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `query_planner`  

---

## 1. Abstract Point-in-Time Snapshot Architecture

GaxFS defines snapshots as **immutable, persistent point-in-time views** over object graph states without duplicating payload data.

### 1.1 Snapshot Creation Semantics
Creating a snapshot captures a consistent state across:
- **Object Store State:** Immutable payload extents and object headers.
- **Namespace View Projections:** Active directory, tag, workspace, and collection mappings.
- **Metadata Dictionaries & Attributes:** User attributes, schemas, and content types.
- **Authoritative Relationship Edges:** Directed graph links (`GeneratedFrom`, `DependsOn`, `References`).

In the v1 `gax_storage_engine`, snapshot creation executes as a constant-time operation by freezing the active Copy-on-Write root pointer without copying underlying data blocks.

---

## 2. Generic `SnapshotProvider` Architecture

Snapshot management is abstracted behind a generic architectural interface: **`SnapshotProvider`**.

```rust
pub trait SnapshotProvider {
    /// Captures a consistent point-in-time snapshot of the target scope
    fn create_snapshot(&mut self, scope_handle: &CapabilityHandle, name: &str) -> Result<GaxObjectId, SnapshotError>;
    
    /// Deletes a historical snapshot and releases associated live references
    fn delete_snapshot(&mut self, snapshot_id: GaxObjectId) -> Result<(), SnapshotError>;
    
    /// Atomically rolls back filesystem state to a historical snapshot
    fn rollback_snapshot(&mut self, target_scope: &CapabilityHandle, snapshot_id: GaxObjectId) -> Result<(), SnapshotError>;
    
    /// Creates a new, independent writable clone branching from a snapshot
    fn create_clone(&mut self, snapshot_id: GaxObjectId, new_name: &str) -> Result<(GaxObjectId, CapabilityHandle), SnapshotError>;
}
```

### Provider Implementations:
- **`gax_storage_engine` (Default v1 Implementation):** Native Copy-on-Write storage engine snapshots.
- **Pluggable Future Providers:** Remote snapshots, replicated snapshot engines, cloud-backed storage snapshots, and distributed snapshot providers.

---

## 3. Core Architectural Invariants

> **Invariant 1 — Snapshot Immutability:**  
> Once committed, a snapshot is strictly immutable. Applications cannot modify snapshot contents. Subsequent filesystem modifications create new storage state without altering existing snapshots.
> 
> **Invariant 2 — Writable Clones as Independent Derived Objects:**  
> Writable clones are new filesystem objects derived from a snapshot baseline. The originating snapshot remains immutable; future modifications affect only the clone.
> 
> **Invariant 3 — Atomic Rollback Semantics:**  
> Rollback restores a previously captured filesystem state through an atomic transition. The underlying storage engine determines how this is achieved.
> 
> **Invariant 4 — Live Reference Preservation:**  
> Objects referenced by active snapshots constitute live references. Physical block destruction (`Destroy`) cannot reclaim an object while it is referenced by a snapshot root pointer.

---

## 4. Event Integration & Secondary State Synchronization

All snapshot operations (creation, deletion, atomic rollback, and clone creation) publish authoritative event records (`SnapshotCreated`, `SnapshotDeleted`) to `GaxFsEventLog`. 
- Ring-3 indexing services (`search_server`, `knowledge_server`, `query_planner`) derive snapshot-related index states **exclusively through event replay**, maintaining complete provider independence.
