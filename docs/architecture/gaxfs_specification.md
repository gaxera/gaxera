# GaxFS Architecture Specification

> **Status:** Authoritative Architectural Specification  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Filesystem Architecture (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `libgaxera::vfs`, `gaxera-abi`  

---

## 1. Object Model

### 1.1 Object Definition
In GaxFS, storage is not organized into untyped sector streams. Storage consists of typed, versioned, capability-secured **`GaxObject`** entities.

Every object comprises:
1. **Header Metadata:** Immutable 128-bit Object ID, generation counter, object type, creation timestamp, and owner Resource Domain.
2. **First-Class Attributes:** Structured key-value metadata dictionary (`author`, `content_type`, `tags`, `custom_attributes`).
3. **Relationship Graph Edges:** Outgoing directed edges linking this object to other objects (`generated_from`, `depends_on`, `references`, `belongs_to`).
4. **Payload Stream / Storage Extents:** Raw data payload allocated as Copy-on-Write extents or inline data blocks.

### 1.2 Object Types
```rust
#[repr(u16)]
pub enum GaxObjectType {
    RegularData = 1,     // Unstructured or structured file data payload
    Directory = 2,       // Mapping from UTF-8 names to (GaxObjectId, CapabilityRights)
    SymbolicLink = 3,    // Capability-relative path reference
    ObjectStream = 4,    // Named secondary attribute payload stream
    Snapshot = 5,        // Read-only point-in-time tree snapshot root
    Relationship = 6,    // Explicit directed graph edge between two objects
    SchemaDefinition = 7,// Extended user-defined attribute validation schema
}
```

### 1.3 Object Identity Generation
Objects are identified by a 128-bit Universally Unique Identifier (`GaxObjectId`):
- **High 64 bits:** Monotonic epoch timestamp + node allocation domain ID.
- **Low 64 bits:** Cryptographically secure CSPRNG pseudo-random sequence.
- **Invariant:** `GaxObjectId` is globally unique across time, storage pools, and network nodes.

### 1.4 Immutability vs. Mutability & Evolution
- **Immutable State:** `GaxObjectId`, creation timestamp, owner domain, and historical snapshot versions are strictly immutable.
- **Mutable State:** Data payload extents, user attributes, and relationship graph links are mutable via Copy-on-Write (CoW) allocation. Modifying a mutable payload allocates new extents, increments the object generation counter, and updates the parent root in a single atomic transaction.

---

## 2. Namespace Model

### 2.1 Namespace Architecture vs. Access Authority
GaxFS makes a fundamental distinction between the **Object Namespace Graph** and **Access Authority**:
- **Global Object Namespace:** Objects can be assigned human-readable paths (`/projects/gaxera/src/main.rs`) within directory trees.
- **Capability-Mediated Authority:** Knowing a path string grants ZERO access right. To open or modify an object, a process MUST present a capability handle (`Handle` / `ObjectId`) referencing the target object or an enclosing directory with sufficient capability rights.

```
       [init (Root Domain)]
                │ (Full Root Capability)
                ▼
       [/ (Root Directory)]
         ├── /bin (ReadOnly Capability -> Process A)
         └── /data/user (ReadWrite Capability -> Process B)
```

### 2.2 Directory Hierarchy & Path Resolution
- Directories map UTF-8 name strings to `(GaxObjectId, MinimumRequiredRights)`.
- Path resolution (`resolve_path(start_dir_handle, path_string)`) is executed by `vfs_server` in Ring 3.
- Resolution walks directory trees strictly relative to `start_dir_handle`. If a process attempts path traversal (`../../etc/passwd`) past the boundary of its starting handle, resolution fails with `AccessDenied`.

### 2.3 Mount Points & Volume Isolation
Volumes (disk partitions, remote network stores, memory disks) are mounted into directory nodes using **Capability Mount Points**. Mounting a volume attaches its root `GaxObjectId` to a directory entry without exposing physical device drivers to client applications.

---

## 3. Capability Integration

### 3.1 Native Filesystem Capability Rights
Unix permission bits (`rwxrwxrwx`, `chown`, `chmod`) are replaced by native GaxFS Capability Rights defined in `GaxFsRights`:

```rust
pub struct GaxFsRights {
    pub const READ: u32             = 1 << 0; // Read data payload extents
    pub const WRITE: u32            = 1 << 1; // Overwrite data payload extents
    pub const APPEND: u32           = 1 << 2; // Append data to payload end
    pub const EXECUTE: u32          = 1 << 3; // Execute binary payload
    pub const ENUMERATE: u32        = 1 << 4; // List directory entries
    pub const SNAPSHOT: u32         = 1 << 5; // Create point-in-time snapshot
    pub const SHARE: u32            = 1 << 6; // Derive & delegate capabilities to child tasks
    pub const DELETE: u32           = 1 << 7; // Unlink / delete object
    pub const MODIFY_METADATA: u32  = 1 << 8; // Edit attributes and metadata
    pub const CREATE_CHILDREN: u32  = 1 << 9; // Create sub-directories/files
}
```

### 3.2 Rights Narrowing & Revocation
- **Rights Narrowing:** A process holding a `ReadWrite` capability to a project folder can derive a `ReadOnly` handle and pass it to an untrusted plugin. The plugin cannot write to or delete the folder.
- **Instant Revocation:** When the parent process revokes a capability handle, the kernel invalidates the capability lineage tree. Any subsequent IPC request from the plugin using that handle fails instantly.

---

## 4. Storage Engine

### 4.1 Copy-on-Write (CoW) B+Tree Architecture
GaxFS employs a 100% Copy-on-Write (CoW) B+Tree engine for all metadata, extent trees, and directory indexes:
- **No In-Place Overwrites:** Existing disk blocks are never overwritten in place. Modifying a block allocates a new block from free space and writes the updated data.
- **Path Cascading:** Parent tree nodes pointing to the modified block are allocated and updated up to the tree root.

```
       [Superblock Gen N]                 [Superblock Gen N+1]
             │                                   │
             ▼                                   ▼
        [Root Node A]                       [Root Node A']
        /          \                       /          \
  [Leaf B]       [Leaf C] (Modified) ──> [Leaf B]     [Leaf C'] (New Block)
```

### 4.2 Atomic 256-Bit Superblock Transactions
- GaxFS maintains dual active superblocks at fixed physical offsets.
- Transaction commit sequence:
  1. Write all modified data extents and CoW tree nodes.
  2. Issue hardware flushing fence (`FLUSH_CACHE`).
  3. Update inactive Superblock with new B+Tree root physical address, incremented generation `N+1`, 256-bit integrity checksum (currently a rolling mixing function in `integrity.rs`; BLAKE3 is the target for Initiative `v1.4`), and atomic commit timestamp.
  4. On reboot, the storage engine selects the superblock with the highest valid generation and matching checksum. Crash consistency is 100% guaranteed.

### 4.3 Extent Allocation & Free-Space Bitmap
- **Extents:** Data is stored as contiguous extent runs `(logical_offset, physical_block_start, block_count)`.
- **Free Space:** Managed via a CoW Segmented Bitmap B+Tree, supporting $O(\log N)$ extent allocation and power-of-two alignment matching DMA boundary constraints (`ContiguousFrame`).

---

## 5. Metadata Model

### 5.1 First-Class Structured Metadata
Unlike POSIX filesystems that relegate extended attributes to optional sidecars (xattrs), GaxFS treats metadata as a first-class citizen embedded directly in the object header:

```rust
pub struct GaxMetadataHeader {
    pub author: String,             // Human or process creator identity
    pub project: String,            // Associated workspace/project identifier
    pub content_type: String,       // MIME/Mantra type (e.g. "text/x-rust", "image/png")
    pub creation_time: u64,         // Nanoseconds since UNIX epoch
    pub modification_time: u64,     // Nanoseconds since UNIX epoch
    pub tags: Vec<String>,          // Categorical tags (e.g. ["source", "v0.9.3", "kernel"])
    pub custom_attributes: BTreeMap<String, Value>, // Arbitrary typed JSON/BSON values
}
```

### 5.2 Named Attribute Streams
Objects support named secondary payload streams (`GaxObjectType::ObjectStream`). Applications can attach thumbnail previews, compilation metrics, or compiler IR directly to a source file without modifying the primary data payload or using external sidecar files.

---

## 6. Relationships

### 6.1 First-Class Graph Edges
GaxFS implements native graph relationships between objects stored directly in object metadata headers:

```rust
pub enum RelationType {
    GeneratedFrom,  // Output binary generated from source code file
    DependsOn,      // Application executable depends on dynamic library
    References,     // Document references image object
    Contains,       // Archive/Directory contains child object
    BelongsTo,      // Document belongs to workspace project
    RelatedTo,      // Generic semantic relationship
}
```

### 6.2 Directed Graph Integrity
- When object $A$ links to object $B$ with `GeneratedFrom`, GaxFS stores the directed edge `(A -> B, GeneratedFrom)`.
- Object relationship edges are stored inside the GaxFS object header and indexed by `gaxfs_server`. When object $B$ is moved or renamed, relationships remain intact because edges reference immutable 128-bit `GaxObjectId`s, not volatile path strings.

---

## 7. Snapshots

### 7.1 Instant Point-in-Time Tree Snapshots
Because GaxFS uses CoW B+Trees, creating a snapshot is an $O(1)$ constant-time operation:
1. Increment reference counter on the root B+Tree node of the target directory/volume.
2. Create a new `GaxObjectType::Snapshot` root pointing to the same B+Tree root physical block address.
3. Subsequent writes to the volume allocate new CoW blocks, leaving historical snapshot blocks 100% untouched.

### 7.2 Granular Per-Directory & Per-Project Snapshots
Snapshots are not restricted to entire disk volumes:
- **Per-Directory Snapshots:** A developer can snapshot `/projects/gaxera` before a major refactor.
- **Instant Rollback:** Restoring a snapshot simply updates the directory root pointer to the historical snapshot block address.
- **Zero Space Overhead:** A new snapshot occupies zero additional disk space until modifications occur.

---

## 8. Integrity

### 8.1 256-Bit Cryptographic Checksums
- **Metadata Checksums:** Every B+Tree metadata block contains a 256-bit BLAKE3 checksum of its payload.
- **Data Checksums:** Optional BLAKE3 data checksums verify data payload integrity.
- **Self-Diagnostic Bitrot Detection:** On every read, `gaxfs_server` recalculates block checksums. If a bitflip or block corruption is detected, the read returns `CorruptedBlock`.

### 8.2 Future Self-Healing & Redundancy
When operating on mirrored or RAID storage pools, detecting a checksum failure automatically triggers self-healing by reading the correct block from a mirror replica and rewriting the corrupted block.

---

## 9. Performance

### 9.1 NVMe & SSD Optimization
- **Page-Aligned Allocation:** Extents are aligned to 4 KiB physical NVMe page boundaries and 2 MiB erase block boundaries, maximizing flash controller throughput.
- **Batch Allocation:** Extent allocations allocate large contiguous runs to minimize B+Tree fragmentation and flash write amplification.

### 9.2 Fast-Path Deduplication & Cloning
- **CoW File Cloning:** Copying a file (`clone_file(src, dst)`) increments extent reference counters and creates a duplicate metadata header in $O(1)$ time without copying data bytes.
- **Background Deduplication:** `gaxfs_server` scans extent checksums in the background to merge duplicate data extents.

---

## 10. Extensibility

### 10.1 On-Disk Format Versioning & Feature Flags
The GaxFS superblock contains explicit feature flags:

```rust
pub struct GaxFsSuperblock {
    pub magic: [u8; 8],             // b"GAXFS\0\1\0"
    pub version_major: u16,         // 1
    pub version_minor: u16,         // 0
    pub incompat_features: u64,     // Required features for write/read
    pub compat_features: u64,       // Optional backward-compatible features
    pub ro_compat_features: u64,    // Features safe for read-only mount
}
```

### 10.2 Forward & Backward Compatibility
If a future version introduces a new feature flag (e.g. 512-bit post-quantum checksums in `incompat_features`), an older GaxFS driver cleanly refuses to mount the volume read-write, preventing volume corruption while preserving read access if marked `ro_compat`.

---

## 11. Compatibility

### 11.1 Isolated POSIX & Windows Compatibility Layer
Legacy software requiring standard C/POSIX file APIs (`open`, `read`, `write`, `stat`, `unlink`, `chmod`) is supported via the Ring-3 **`libgaxera::vfs`** compatibility wrapper:

```
[Legacy POSIX App] ──> [libgaxera::vfs Emulation] ──(IPC)──> [vfs_server] ──> [GaxFS Object Engine]
```

### 11.2 POSIX Mapping Rules
1. **Paths:** POSIX paths (`/usr/bin/gcc`) are resolved relative to the process's root directory capability handle.
2. **File Descriptors:** Integer FDs (`3`, `4`, `5`) map to thread-local capability handles in `libgaxera::vfs`.
3. **Permissions:** `chmod(0755)` maps to updating standard metadata attributes without altering underlying GaxFS capability rights.

---

## 12. Search & Knowledge Architecture

### 12.1 Event-Driven Lifecycle Architecture
GaxFS **does not embed a search engine database or AI vector database inside disk blocks**. Storage engine stability must never be compromised by complex indexing engines.

Instead, GaxFS implements an **Event-Driven Lifecycle Emission System**:

```
[GaxFS Storage Engine]
       │
       ├── Emits Event: ObjectCreated { id, type, metadata }
       ├── Emits Event: ObjectModified { id, extent_delta }
       └── Emits Event: RelationshipLinked { src, dst, relation }
       │
       ▼
[Ring-Buffer Event Stream (GaxFsEventLog)]
       │
       ├── Consumed by ──> [Ring-3 Search / Spotlight Service]
       ├── Consumed by ──> [Ring-3 Vector Embedding Service (TurboQuant)]
       └── Consumed by ──> [Ring-3 Knowledge Graph Engine (TurboVec)]
```

### 12.2 TurboQuant & TurboVec Integration for Native Semantic Search
To provide native, instant OS-wide semantic search without ballooning storage or memory footprint, GaxFS integrates with **TurboQuant** and **TurboVec** at the Ring-3 service layer:

1. **TurboQuant Sub-Byte Vector Compression:**
   - High-dimensional vector embeddings (1536-dim float32 vectors generated from file contents) are compressed by the Ring-3 `search_server` using **TurboQuant** polar quantization down to 1-bit or 2-bit representations (96 to 192 bytes per file).
   - Compressed vector signatures are attached directly to object headers as named secondary attribute streams (`GaxObjectType::ObjectStream`).

2. **TurboVec Vector Search:**
   - The unprivileged `search_server` executes TurboVec vector search over TurboQuant-compressed object streams (prototype uses scalar similarity loops; AVX-512 / AVX2 / ARM NEON SIMD intrinsics are targeted for post-v1 optimization).
   - Evaluates similarity distance across 100,000+ filesystem objects in `< 1 millisecond` on host CPU hardware without GPU dependencies.

3. **Architectural Guarantee:**
   - Core GaxFS disk operations remain 100% independent of vector processing.
   - If the AI indexing service is paused or terminated, file storage, reading, and writing continue at 100% efficiency with zero data hazard.
