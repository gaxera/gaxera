# GaxFS Object Model Specification

> **Status:** Authoritative Architecture Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `libgaxera::vfs`, `gaxera-abi`  

---

## 1. First-Principles Object Architecture

In GaxFS, storage is organized around first-class **`GaxObject`** instances rather than untyped flat sector streams.

### 1.1 Object Definition
A `GaxObject` is a self-describing, capability-secured storage entity comprising:
1. **Permanent Identity (`GaxObjectId`):** Standard 128-bit RFC 9562 UUIDv7 identifier.
2. **Immutable Identity Metadata:** Creation timestamp, owner domain ID, and initial creation epoch.
3. **Evolvable Schema & Versioned Metadata:** Extensible key-value metadata attributes (`author`, `project`, `content_type`, `tags`, schema descriptors, custom attributes).
4. **Authoritative Graph Relationships:** Directed edges linking to other `GaxObjectId`s (`generated_from`, `depends_on`, `references`, `contains`, `belongs_to`).
5. **Data Payload Extents:** Copy-on-Write data payload extents or inline data blocks.

---

## 2. Object Identity (`GaxObjectId`) — Standard UUIDv7 Strategy

`GaxObjectId` is the canonical identifier for every object stored in GaxFS. Its default encoding uses standard **UUIDv7 (RFC 9562)**, providing globally unique, time-ordered 128-bit identifiers. The architecture depends strictly on the `GaxObjectId` abstraction, not on any specific encoding.

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct GaxObjectId {
    /// 128-bit standard UUIDv7 encoding (RFC 9562)
    pub bytes: [u8; 16],
}
```

### Identity Invariants & Locality Properties:
1. **Location Independence:** Moving, renaming, or re-parenting an object in namespace views **never alters its `GaxObjectId`**.
2. **Encouraged Index Locality:** Time-ordered UUIDv7 structure encourages write and index locality, clustering recently created objects in adjacent index nodes.
3. **Efficient Direct Lookup:** The storage engine provides efficient location-independent object lookup by `GaxObjectId`.

---

## 3. Extensible Object Type Architecture

GaxFS avoids fixed enumerations, providing a core set of built-in object categories alongside an extensible Ring-3 type registry (**`GaxObjectTypeRegistry`**):

```rust
#[repr(u16)]
pub enum CoreGaxObjectType {
    Document = 1,        // Text, Markdown, PDF, Office documents
    Image = 2,           // PNG, JPEG, SVG, raster/vector images
    Video = 3,           // MP4, WebM, video stream payloads
    Executable = 4,      // GAXERA ELF binaries, dynamic libraries
    SourceCode = 5,      // Rust, C, Python source files
    Project = 6,         // Workspace directory container
    ArchiveBundle = 7,   // Tar, Zip, GAXFS archive bundles
    Dataset = 8,         // CSV, Parquet, Tensor matrices, KV datasets
    Configuration = 9,   // TOML, JSON, System configuration schemas
    ModelWeights = 10,   // ML/AI model parameters, neural network weights
    VectorIndexData = 11,// TurboQuant vector signatures, index data
    Stream = 12,         // Named secondary payload stream
    Snapshot = 13,       // Read-only point-in-time tree snapshot root
    CustomRegistered = 14,// User/App-defined type registered in GaxObjectTypeRegistry
}
```

---

## 4. Relationship Semantics & Authoritative Ownership

Relationship graph edges (`GeneratedFrom`, `DependsOn`, `References`, `Contains`, `BelongsTo`) stored inside `GaxObject` headers are **authoritative filesystem metadata**.

### Architectural Consequences:
1. **First-Principles Design Decision:** Graph relationships are guaranteed by GaxFS itself, rather than delegated to third-party databases.
2. **Transactional Consistency:** Relationship updates participate directly in `gaxfs_server` Copy-on-Write transactions, committing atomically alongside payload extents and metadata attributes.
3. **Integrity Preservation:** Snapshots, event log replay checkpoints, replication streams, and crash recovery preserve relationship graph edges with 100% fidelity.
4. **Out-of-Band Graph Indexing:** Emitted via `GaxFsEventLog` and consumed out-of-band by Ring-3 services (`knowledge_server`) to build fast graph query indexes without blocking filesystem IO.

---

## 5. Complete Object Lifecycle & Destruction Semantics

GaxFS formally defines a 7-stage deterministic object lifecycle:

```
[1. Create] ──> [2. Modify] ──> [3. Commit] ──> [4. Snapshot]
                                                     │
[7. Destroy] <── [6. Recover] <── [5. Delete] <──────┘
```

> **Storage Tiering Policy (Orthogonal):** Archiving and compression are **storage tiering policies**, not lifecycle states. An object can be compressed via ZSTD or moved to cold flash storage while remaining 100% active in state.

### 5.1 Lifecycle Conceptual Distinctions:
- **Logical Deletion:** Unlinking an object from namespace views (`DirectoryView`, `TagView`).
- **Reachability:** Whether an object can be resolved by new path lookups. Logically deleted objects become unreachable to new tasks.
- **Live References:** Active capability handles held by running processes, snapshot root pointers, or mandatory structural relationship edges. Existing processes holding open capability handles may continue accessing unreachable objects until their handles are released.
- **Physical Destruction (`Destroy`):** Storage block reclamation occurring ONLY when all live references reach zero.

### 5.2 Strict Reference-Counted Destruction Preconditions (`Destroy`)
Physical block reclamation occurs **ONLY** after `gaxfs_server` verifies that all live references have reached zero:
1. **Zero Active Namespace Links:** Object is unlinked from all directory, tag, workspace, and collection views.
2. **Zero Snapshot Root Pointers:** Object is not referenced by any read-only snapshot root.
3. **Zero Open Capability Handles:** All processes holding an open capability handle to the object in `CapabilitySpace` have closed their handle or terminated.
4. **Zero Structural Graph Dependencies:** Object has no incoming mandatory non-cascading relationship graph links.
