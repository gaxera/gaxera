# GaxFS On-Disk Format & Storage Engine Specification

> **Status:** Authoritative Architecture Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`  

---

## 1. Abstract Storage Architecture vs. Current v1 Layout

GaxFS distinguishes between its **Abstract Storage Architecture** and the **v1 Concrete Physical Layout**.

### 1.1 Abstract Storage Concepts
The storage engine architecture defines six core abstractions regardless of physical disk placement:
1. **Superblock:** Global volume metadata, transaction generation counter, and volume configuration.
2. **Allocation Metadata:** Free-space tracking and extent allocation structures.
3. **Object Index:** Location-independent lookup index mapping `GaxObjectId` to extent descriptors.
4. **Extent Storage:** Copy-on-Write payload extent blocks.
5. **Private Storage Journal (`gax_storage_journal`):** Private transaction commit and recovery log.
6. **Feature Metadata:** Compatible, read-only compatible, and incompatible capability flags.

---

## 2. Current (v1) Physical On-Disk Layout

In the v1 implementation, physical disk storage is formatted into block boundaries (default 4 KiB block size, configurable per deployment):

```
[Block 0: Boot Header]
[Block 1: Primary Superblock (Gen N)]
[Block 2: Backup Superblock (Gen N-1)]
[Block 3..M: Free-Space Allocation Index]
[Block M+1..K: Object Index Root]
[Block K+1..J: Private Storage Journal (gax_storage_journal)]
[Block J+1..N: Data Payload Extents]
```

### 2.1 v1 Superblock Structure (`GaxFsSuperblock`)

```rust
#[repr(C, packed)]
pub struct GaxFsSuperblock {
    pub magic: [u8; 8],             // b"GAXFS\0\1\0"
    pub version_major: u16,         // 1
    pub version_minor: u16,         // 0
    pub generation: u64,            // Monotonic transaction generation counter
    pub block_size: u32,            // Physical block size (v1 default 4096 bytes)
    pub total_blocks: u64,          // Total block count
    pub free_blocks: u64,           // Remaining free blocks
    pub object_index_root: u64,     // Offset of Object Index root
    pub allocation_index_root: u64, // Offset of Free-Space Allocation Index
    pub storage_journal_head: u64,  // Offset of private storage journal (gax_storage_journal)
    pub incompat_features: u64,     // Incompatible capability flags
    pub compat_features: u64,       // Compatible capability flags
    pub ro_compat_features: u64,    // Read-only compatible capability flags
    pub commit_timestamp: u64,      // Commit time in nanoseconds since UNIX epoch
    pub checksum: [u8; 32],         // Cryptographic integrity checksum of superblock (BLAKE3 v1 default)
}
```

### 2.2 Dual Superblock Atomic Commit Sequence
1. Write all new Copy-on-Write (CoW) extent payload blocks and index nodes.
2. Issue hardware storage cache flush (`FLUSH_CACHE`).
3. Update inactive Superblock (alternating between Block 1 and Block 2) with generation `N+1`, updated index roots, timestamp, and 256-bit cryptographic checksum.
4. Issue final cache flush.
5. **Transactional Crash Consistency Guarantee:** On boot, `gax_storage_engine` reads both superblocks, verifies cryptographic checksums, and selects the valid superblock with the highest generation number. Abrupt power loss mid-write causes zero metadata corruption.

---

## 3. Capability-Based Feature Flags

Feature flags describe volume capabilities rather than hardcoded algorithm names:

```rust
pub struct GaxFsFeatureFlags;

impl GaxFsFeatureFlags {
    // Incompatible Features (Refuse mount if unsupported)
    pub const INCOMPAT_PAYLOAD_COMPRESSION: u64    = 1 << 0;
    pub const INCOMPAT_CONTENT_DEDUPLICATION: u64  = 1 << 1;
    pub const INCOMPAT_ADVANCED_INTEGRITY: u64     = 1 << 2;

    // Read-Only Compatible Features (Safe to mount read-only if unsupported)
    pub const RO_COMPAT_VECTOR_QUANTIZATION: u64   = 1 << 0;
    pub const RO_COMPAT_GRAPH_RELATIONSHIPS: u64   = 1 << 1;

    // Compatible Features (Safe to read/write if unsupported)
    pub const COMPAT_EXTENDED_ATTRIBUTES: u64      = 1 << 0;
}
```

---

## 4. Cryptographic Integrity & Compression Framework

1. **Cryptographic Integrity Verification:** GaxFS requires 256-bit cryptographic checksums over all metadata and data extent blocks. BLAKE3 is the default v1 checksum implementation and can be updated via capability flags.
2. **Technology-Neutral Compression:** Payload extent compression is managed by `CompressionProvider`. On-disk extent descriptors record compression flags required for decoding rather than coupling storage architecture to specific algorithms.

---

## 5. Evaluation of Content-Addressable Storage (CAS)

GaxFS distinguishes **Object Identity**, **Storage Allocation**, and **Optional Content-Addressable Deduplication**:
- **Object Identity:** `GaxObjectId` is strictly location-independent and timestamp-ordered (RFC 9562 UUIDv7).
- **Optional Deduplication Optimization:** Content-Addressable Storage (CAS) deduplication is an optional background optimization for read-only snapshot blocks. Deduplication is never required for correctness.

---

## 6. Storage Engine Boundaries (`gax_storage_engine`)

`gax_storage_engine` is a generic, reusable storage engine.

```
┌─────────────────────────────────────────────────────────────┐
│ gax_storage_engine OWNS:                                    │
│  - Block allocation & free-space management                 │
│  - Storage persistence & hardware cache flushes             │
│  - Transaction commit protocols & dual superblocks          │
│  - Private storage journal recovery (gax_storage_journal)   │
│  - Cryptographic integrity checksum verification            │
│  - Copy-on-Write (CoW) page & extent management             │
├─────────────────────────────────────────────────────────────┤
│ gax_storage_engine DOES NOT OWN:                            │
│  - Object types, schemas, & business semantics              │
│  - Namespace views & path resolutions                       │
│  - User metadata dictionaries & graph relationships         │
│  - Capability security enforcement                          │
│  - Query planning & GaxQL execution                         │
└─────────────────────────────────────────────────────────────┘
```

---

## 7. Storage Device Independence (`StorageDeviceProvider`)

`gax_storage_engine` does not depend directly on physical block hardware. All storage I/O is abstracted through a generic trait: **`StorageDeviceProvider`**.

```rust
pub trait StorageDeviceProvider {
    /// Reads blocks from persistent storage into a buffer
    fn read_blocks(&self, block_offset: u64, count: u32, buf: &mut [u8]) -> Result<(), StorageDeviceError>;
    
    /// Writes blocks to persistent storage from a buffer
    fn write_blocks(&self, block_offset: u64, count: u32, buf: &[u8]) -> Result<(), StorageDeviceError>;
    
    /// Flushes hardware/volatile storage write caches to guarantee durability
    fn flush(&self) -> Result<(), StorageDeviceError>;
    
    /// Reports storage geometry, allocation unit size, and total units
    fn geometry(&self) -> StorageGeometry;
}
```

### 7.1 Provider Implementations & Decoupled Layering Stack

```
[StorageDeviceProvider] ── (virtio_block_server, nvme_server, NVDIMM, RAM, Network)
           │
           ▼
[gax_storage_engine]    ── (CoW Page Allocator, Checksums, Journal, Recovery)
           │
           ▼
[gaxfs_server]          ── (Object Store, Metadata Dictionaries, Graph Edges)
           │
           ▼
[vfs_server]            ── (Virtual File System & Namespace Router)
           │
           ▼
[Applications]
```

- **Current v1 Implementations:** `nvme_server`, `virtio_block_server`, SATA SSD / HDD drivers.
- **Pluggable Future Implementations:** Persistent memory (NVDIMM / Optane), RAM-backed storage, network-backed storage pools, distributed storage backends.
- **Architectural Boundary:** `gax_storage_engine` depends only on abstract `StorageDeviceProvider` capabilities (read, write, flush, geometry) rather than any particular physical hardware interface.
