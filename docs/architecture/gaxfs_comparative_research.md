# GaxFS Comparative Storage System Research

> **Status:** Authoritative Comparative Research Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Filesystem Architecture (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`  

---

## 1. Executive Overview

To design GaxFS from first principles, we conducted an exhaustive architectural analysis of 14 landmark storage systems spanning classic OS filesystems, modern CoW filesystems, content-addressable version control engines, and embedded transactional key-value databases.

The goal of this research is **not to copy implementations**, but to extract core architectural principles, identify historical design trade-offs, and justify every major decision in GaxFS from first principles.

---

## 2. Deep System Analysis (14 Storage Architectures)

### 2.1 FAT32
- **Strengths:** Simplicity, minimal memory footprint, broad hardware compatibility.
- **Weaknesses:** Lacks journaling and crash protection; file size limit (4 GiB); cluster allocation fragmentation; no metadata attribute model.
- **Architectural Inspirations:** Simple, explicit cluster allocation tracking.
- **Trade-Offs to Avoid:** Untyped directory blocks without checksums; lack of crash consistency.

### 2.2 exFAT
- **Strengths:** Large file support, optimized allocation bitmaps for flash storage.
- **Weaknesses:** Lacks Copy-on-Write and journaling; susceptible to metadata corruption on abrupt power loss.
- **Architectural Inspirations:** Contiguous cluster allocation flags for flash writes.
- **Trade-Offs to Avoid:** Absence of cryptographic checksums and metadata redundancy.

### 2.3 ext4
- **Strengths:** High reliability, extent-based block mapping, efficient HTree directory indexing.
- **Weaknesses:** In-place metadata overwrites require POSIX journaling (JBD2); fixed inode table layout limits metadata scaling; lacks native snapshotting.
- **Architectural Inspirations:** Extent tree representation for contiguous file layouts.
- **Trade-Offs to Avoid:** Fixed pre-allocated inode tables; reliance on in-place block updates.

### 2.4 XFS
- **Strengths:** High parallel allocation scalability via Allocation Groups (AGs), 64-bit inode indexing, B+Tree extents.
- **Weaknesses:** Memory overhead for small workloads; historical vulnerability to unwritten zero-filled extents on abrupt crashes.
- **Architectural Inspirations:** Decoupled allocation groups for multi-core parallelism across CPUs.
- **Trade-Offs to Avoid:** Deferred allocation flush windows that leave unwritten zero-filled extents on crash.

### 2.5 ZFS
- **Strengths:** End-to-end 256-bit checksumming (Merkle tree integrity), pooled storage (ZPOOL), instant CoW snapshots, RAID-Z redundancy, ARC cache.
- **Weaknesses:** Heavy memory footprint for ARC/dedup; rigid POSIX ACL/UID legacy model; high CPU overhead.
- **Architectural Inspirations:** Merkle tree checksum hierarchy; dual-superblock atomic generation commit; snapshotting.
- **Trade-Offs to Avoid:** Monolithic in-kernel execution model; mandatory high memory footprint for basic single-node operations.

### 2.6 Btrfs
- **Strengths:** Native Copy-on-Write B-trees, subvolumes, instant snapshots, extents, background scrubbing.
- **Weaknesses:** B-tree lock contention under heavy parallel write workloads; complex recursive B-tree balancing failure modes.
- **Architectural Inspirations:** Extensible key-value item representation for metadata and extents.
- **Trade-Offs to Avoid:** Over-complex unified structures where metadata and data extent locks choke multi-core throughput.

### 2.7 APFS (Apple File System)
- **Strengths:** Optimized for Solid State Drives (SSDs), instant file cloning, fast directory sizing, space sharing across volumes, strong encryption.
- **Weaknesses:** Proprietary closed spec; heavy reliance on POSIX path resolution; single-threaded lock bottlenecks during large snapshot deletions.
- **Architectural Inspirations:** Instant file cloning via extent reference counting; native flash-aware page alignment.
- **Trade-Offs to Avoid:** Coupling filesystem encryption directly to user login passwords without capability token derivation.

### 2.8 NTFS
- **Strengths:** Master File Table (MFT) record structure, named data streams (ADS), journaling, fine-grained ACLs.
- **Weaknesses:** MFT fragmentation over time; high complexity; legacy POSIX/DOS dual-name debt.
- **Architectural Inspirations:** First-class named secondary attribute streams attached to primary objects.
- **Trade-Offs to Avoid:** Complex ACL inheritance schemes evaluated at path resolution time.

### 2.9 ReFS (Resilient File System)
- **Strengths:** Metadata integrity, automatic error detection via checksums, proactive scrubbing, real-time mirror repair.
- **Weaknesses:** Storage overhead; removed support for bootable volumes; slow small-file performance.
- **Architectural Inspirations:** Strict separation of data integrity metadata from volume boot blocks.
- **Trade-Offs to Avoid:** Inability to serve as a primary bootable system filesystem.

### 2.10 F2FS (Flash-Friendly File System)
- **Strengths:** Append-only log-structured filesystem (LFS) designed for NAND flash geometry, node address table (NAT), cold/hot data separation.
- **Weaknesses:** Garbage collection (GC) pauses under high disk utilization ($>90\%$), complex multi-head log management.
- **Architectural Inspirations:** Separation of hot metadata blocks from cold file payload blocks to reduce flash wear.
- **Trade-Offs to Avoid:** High latency spikes caused by synchronous garbage collection sweeps.

### 2.11 NILFS / NILFS2
- **Strengths:** Continuous snapshotting log-structured filesystem, ability to restore filesystem state to any past second.
- **Weaknesses:** Continuous log cleaner chokes disk throughput under write-heavy workloads.
- **Architectural Inspirations:** Time-travel epoch log pointers for snapshot inspection.
- **Trade-Offs to Avoid:** Unbounded log growth without policy-driven snapshot pruning.

### 2.12 Git Object Storage
- **Strengths:** Content-Addressable Storage (CAS), immutable DAG (Directed Acyclic Graph) relationships (`parent`, `tree`, `commit`), deterministic hashing.
- **Weaknesses:** Low performance for large mutable binary payloads; high file handle overhead before packfile consolidation.
- **Architectural Inspirations:** Explicit, first-class directed relationship edges (`GeneratedFrom`, `DependsOn`, `References`) linking stored objects.
- **Trade-Offs to Avoid:** Re-hashing multi-gigabyte data payloads on every minor edit; lack of dynamic extent indexing.

### 2.13 LMDB (Lightning Memory-Mapped Database)
- **Strengths:** Fast single-file Copy-on-Write B+Tree, zero-copy read operations via memory mapping, single-writer multi-reader ACID transactions.
- **Weaknesses:** Limited write scalability due to single global write lock; database size capped by virtual memory address space.
- **Architectural Inspirations:** Clean CoW page allocation logic; zero-copy read paths.
- **Trade-Offs to Avoid:** Single global write lock bottleneck across multi-core systems.

### 2.14 SQLite Storage Engine
- **Strengths:** B-Tree page engine, atomic rollback journal / WAL (Write-Ahead Logging), portable single-file binary format, rigorous test suite.
- **Weaknesses:** User-space database engine not designed for direct block device extent allocation or hardware DMA.
- **Architectural Inspirations:** Structured page payload formatting and transaction rollback verification.
- **Trade-Offs to Avoid:** Embedding a full SQL query engine inside the core block storage layer.

---

## 3. Summary of Architectural Inspirations for GaxFS

| Feature / Principle | Prior Art Origin | GaxFS Authoritative Design Reference |
| --- | --- | --- |
| **Cryptographic Integrity** | ZFS, Btrfs | [GaxFS On-Disk Format](gaxfs_ondisk_format.md) — 256-bit cryptographic checksums via `gax_storage_engine`. |
| **Crash Consistency** | ZFS, APFS, LMDB | [GaxFS Storage Engine](gaxfs_ondisk_format.md) — Dual-superblock generation commits. |
| **Object Relationships** | Git Object Storage | [GaxFS Object Model](gaxfs_object_model.md) — First-class directed graph edges stored directly in object headers. |
| **Capability Security** | seL4, Gaxera | [GaxFS Capability Model](gaxfs_capability_model.md) — Zero ambient path authority; mediated by `CapabilityHandle`. |
| **Event-Driven Stream** | Spotlight, Event Sourcing | [GaxFS Event Model](gaxfs_event_model.md) — Persistent `EventProvider` stream (`GaxFsEventLog`). |
| **Three-Layer Querying** | Modern Query Engines | [GaxFS Indexing Architecture](gaxfs_indexing_architecture.md) — GaxQL AST parsed by Ring-3 `query_planner`. |

---

## 4. Reference Implementation Research & Case Studies

### 4.1 Case Study 1: TurboQuant (Google Research Paper)
- **Source:** Google Research paper (`Amir Zandieh, Majid Daliri, Majid Hadian, Vahab Mirrokni`).
- **Core Insights:** Random orthogonal rotations combined with 1D scalar Lloyd-Max codebooks yield unbiased inner product estimation over vector embeddings.
- **Relevance to GaxFS:** Informed the generic `CompressionProvider` abstraction, providing high-ratio lossy vector quantization for embeddings, telemetry, and numerical waveforms.

### 4.2 Case Study 2: TurboVec (SIMD Vector Engine)
- **Source:** `turbovec` Rust codebase (`.research/turbovec-main`).
- **Core Insights:** SIMD lookup tables (AVX-512/NEON) combined with search-time allowlist filtering execute vector similarity scoring at high speed.
- **Relevance to GaxFS:** Informed the `SemanticIndexProvider` baseline implementation, confirming that capability allowlist filtering can be embedded inside vector scoring loops without privacy leakage.

---

## 5. Cross-Cutting Storage Principles

Across all 14 analyzed storage systems, seven fundamental architectural principles emerged:
1. **Immutable State Improves Recoverability:** Copy-on-Write allocations eliminate in-place overwrites, making crash recovery deterministic.
2. **Stable Object Identity:** Decoupling permanent identity (`GaxObjectId`) from mutable string paths prevents broken links and refactoring failure.
3. **Rebuildable Derived State:** Derived index structures must be 100% rebuildable from authoritative event replay streams.
4. **Verifiable Integrity:** Cryptographic checksums must validate data and metadata at the storage engine boundary.
5. **Extensible Metadata:** Key-value metadata dictionaries avoid rigid pre-allocated attribute tables.
6. **Layered Replaceability:** Storage engines, index backends, and compression providers must remain replaceable behind abstract trait boundaries.
7. **Separation of Concerns:** Storage durability, capability security, query planning, and event streaming must remain decoupled.

---

## 6. Role of Comparative Research in GaxFS Governance

> **Governance Principle:**  
> This comparative research informed—but does not define—the GaxFS architecture. Final authoritative architectural decisions are documented in the GaxFS specification suite and governed through the Architectural Decision Record (ADR) process.
