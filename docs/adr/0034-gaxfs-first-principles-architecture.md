# ADR 0034: GaxFS — First-Principles Architecture & Object Storage Platform

> **Status:** Approved  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `libgaxera::vfs`, `gaxera-abi`, `search_server`, `query_planner`  

---

## 1. Context & Guiding Question

For four decades, operating system filesystems have been designed around legacy Unix/POSIX assumptions:
- Untyped flat byte-stream files (`open()` $\rightarrow$ `read()` raw sector streams).
- Ambient administrative privileges (`root`/`sudo`, UIDs, GIDs, octal mode bits).
- Hierarchical string paths as single monolithic identities (`/home/user/document.pdf`).
- Synchronous polling and manual directory walking for search and indexing.

**The Guiding Question for GaxFS:**
> *"What would a native filesystem look like if we designed it today—from first principles—knowing everything we've learned from decades of storage systems, databases, distributed systems, search engines, AI systems, and modern operating systems?"*

---

## 2. Core Architectural Decisions

### 2.1 Fundamental Primitive: Object Store with Multiple Namespace Views
GaxFS is an **Object Store**, not a traditional POSIX file hierarchy.
- The fundamental storage unit is `GaxObjectId`, the canonical identifier for every object stored in GaxFS. Its default encoding uses standard **UUIDv7 (RFC 9562)**, providing globally unique, time-ordered 128-bit identifiers. The architecture depends strictly on the `GaxObjectId` abstraction, not on any specific encoding.
- Path strings (`/projects/gaxera/src/main.rs`), tag views, workspace views, and dynamic collections are lightweight **Namespace Views** that map to underlying `GaxObjectId`s.
- Renaming or moving an object updates a view entry, but **never changes the object's identity (`GaxObjectId`)**.

### 2.2 Reusable Generic Storage Engine (`gax_storage_engine`)
Persistence, Copy-on-Write (CoW) page allocation, transactions, 256-bit integrity checksums (currently a rolling mixing function; BLAKE3 is the target for production adversarial environments), block cache, and recovery are owned by a generic, reusable Ring-3 storage engine library crate: **`gax_storage_engine`**.
- `gax_storage_engine` owns no filesystem semantics (object types, metadata dictionaries, namespaces, relationships, and capabilities belong exclusively to GaxFS).

#### Threat Model: Storage Integrity
The current checksum implementation is non-cryptographic and intended for trusted block device stacks. For untrusted or adversarial storage backends, upgrading the checksum generator to BLAKE3 or SHA-256 is required.

### 2.3 Strict Architectural Separation: Storage Journal vs Public Event Log
GaxFS maintains a clear conceptual boundary between internal durability and public event emission:
1. **Filesystem Storage Journal (`gax_storage_journal`):** Private to `gax_storage_engine`. Handles transactional durability, dual-superblock generation commits, extent block allocation logging, and hardware cache flush fences.
2. **Public OS Event Log (`GaxFsEventLog`):** Core OS primitive. Exposes zero-copy event publication streams (`ObjectCreated`, `ObjectModified`) to Ring-3 subscribers and stores replayable `EventCheckpointMarker` snapshots for index rehydration, auditing, and telemetry.

### 2.4 Clean Service Layering & Query Planner (`query_planner`)

```
Applications
     │
     ▼
Query Planner (query_planner)
     │
     ▼
vfs_server (Virtual File System & Namespace Router)
     │
     ▼
gaxfs_server (GaxFS Object Engine)
     │
     ├── Emits Event ──> GaxFsEventLog (Public OS Event Stream & Replayable Journal)
     │                        │
     │                        ├── Subscribed by ──> search_server (Full-Text & Vector Search)
     │                        ├── Subscribed by ──> knowledge_server (Knowledge Graph)
     │                        ├── Subscribed by ──> backup_server (Backup Service)
     │                        └── Subscribed by ──> telemetry_server (Audit Service)
     ▼
gax_storage_engine (Generic CoW Storage Engine & Block Cache)
     │
     ▼
Block Device Servers (virtio_block_server, nvme_server)
```

- **Query Planner (`query_planner`):** The Ring-3 planner parses GaxQL ASTs, performs cost estimation, selects required indexes, reorders predicates, intersects candidate sets, and enforces capability authorization filtering.

### 2.5 Authoritative Relationship Ownership
GaxFS explicitly defines directed graph relationships (`GeneratedFrom`, `DependsOn`, `References`, `Contains`, `BelongsTo`) as **authoritative filesystem metadata** embedded directly in `GaxObject` headers.
- **Transactional Consistency:** Relationship updates participate in `gaxfs_server` Copy-on-Write transactions, committing atomically alongside payload extents and metadata attributes.
- **Integrity Preservation:** Snapshots, event log replay, replication, and disaster recovery preserve relationship graph integrity with 100% fidelity.
- **Intentional Design Decision:** This is a deliberate first-principles architectural choice ensuring that object relationships are guaranteed by GaxFS itself without depending on external databases.

### 2.6 Refined Destruction Semantics & Reference-Counted Reclamation
GaxFS distinguishes between **Logical Deletion**, **Reachability**, **Live References**, and **Physical Destruction**:
1. **Logical Deletion:** Unlinking an object from namespace views. The object becomes unreachable for new path resolutions.
2. **Live References:** Processes holding pre-existing open capability handles may continue accessing the unreachable object until their handles are closed.
3. **Physical Destruction:** Occurs ONLY when all live references (namespace links, snapshot root pointers, active capability handles, structural graph dependencies) reach zero, guaranteeing zero dangling references.

### 2.7 Behavioral Specifications over Asymptotic Complexity Guarantees
GaxFS specifications define **required behavioral properties** (efficient location-independent object lookup, scalable namespace resolution, transactional crash consistency) rather than constraining future implementations with rigid asymptotic complexity claims ($O(1)$, $O(\log N)$).

### 2.6 Capability Security Model (Zero Ambient Authority)
- There is no global root filesystem tree accessible via ambient authority.
- Processes interact with GaxFS exclusively through capability handles (`Handle` / `ObjectId`) in their `CapabilitySpace`.
- Path resolution and search queries are filtered so that processes only see objects for which they hold capability authority.

### 2.7 Isolated Compatibility Mapping
Legacy APIs (POSIX `open`/`read`/`write`, Windows Win32 file APIs, macOS APIs) are implemented as pure Ring-3 translation wrappers (`libgaxera::vfs`), keeping native GaxFS clean.

---

## 3. Authoritative Architectural Specification Suite

This ADR is supported by the comprehensive specification suite:
- [docs/architecture/gaxfs_object_model.md](../architecture/gaxfs_object_model.md)
- [docs/architecture/gaxfs_namespace_spec.md](../architecture/gaxfs_namespace_spec.md)
- [docs/architecture/gaxfs_ondisk_format.md](../architecture/gaxfs_ondisk_format.md)
- [docs/architecture/gaxfs_indexing_architecture.md](../architecture/gaxfs_indexing_architecture.md)
- [docs/architecture/gaxfs_event_model.md](../architecture/gaxfs_event_model.md)
- [docs/architecture/gaxfs_capability_model.md](../architecture/gaxfs_capability_model.md)
- [docs/architecture/gaxfs_snapshot_architecture.md](../architecture/gaxfs_snapshot_architecture.md)
- [docs/architecture/gaxfs_compatibility_strategy.md](../architecture/gaxfs_compatibility_strategy.md)
- [docs/architecture/gaxfs_storage_engine_tradeoff_analysis.md](../architecture/gaxfs_storage_engine_tradeoff_analysis.md)
- [docs/architecture/gaxfs_comparative_research.md](../architecture/gaxfs_comparative_research.md)
- [docs/architecture/gaxfs_longterm_roadmap.md](../architecture/gaxfs_longterm_roadmap.md)
