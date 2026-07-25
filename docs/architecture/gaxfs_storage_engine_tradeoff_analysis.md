# GaxFS Storage Engine Architecture Trade-Off Analysis & Recommendation

> **Status:** Authoritative Architectural Trade-Off Analysis  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `search_server`, `knowledge_server`, `query_planner`  

---

## 1. Executive Summary & Problem Definition

A fundamental decision in filesystem architecture is whether to implement:
- **Option A (Monolithic Common Storage Engine):** A single unified storage engine/library used identically by core filesystem services, namespace managers, and all index services.
- **Option B (Autonomous Service Storage Engines):** Complete freedom where every Ring-3 service designs and owns its own storage engine from scratch.
- **Option C (Gaxera Layered Storage Architecture — Recommended):** A generic storage engine (`gax_storage_engine`) providing transactional persistence combined with autonomous, domain-optimized provider services for specialized indexing.

---

## 2. Comparative Evaluation Across 6 Dimensions

| Evaluation Dimension | Option A: Monolithic Common Engine | Option B: Autonomous Service Engines | Option C: Recommended Layered Storage Architecture |
| --- | --- | --- | --- |
| **1. Performance** | **Suboptimal:** Forces specialized index structures into generic page allocations, increasing cache misses and CPU overhead. | **High (In-Memory):** Services use domain-optimized in-memory representations. | **Implementation-Optimized:** Specialized in-memory representations for indexes; zero-copy page buffers for persistence. |
| **2. Maintainability** | **High Initial:** Single storage engine codebase. | **Low:** Duplicate block allocation, transaction management, cryptographic integrity verification, and recovery code across crates. | **High:** Centralized block allocation, cryptographic checksums, and dual-superblock commits in `gax_storage_engine`; services write clean domain serializers. |
| **3. Microkernel Isolation** | **Poor:** Indexing bugs can corrupt core filesystem storage pools if sharing process memory spaces. | **Excellent:** Strict process memory separation. Index service crashes cannot impact core `gaxfs_server`. | **Excellent:** Process boundary isolation enforced via capability IPC and `EventProvider` stream. |
| **4. Crash Recovery & Consistency** | **Complex:** Rolling back a monolithic multi-table database requires complex WAL journals. | **Inconsistent:** Each service has different recovery logic, risking index-data desynchronization on power loss. | **Deterministic Recovery:** `gax_storage_engine` dual-superblock commit guarantees crash consistency. Indexes rebuild deterministically from `EventProvider` replay. |
| **5. Memory Footprint** | **High:** Heavy generalized page caches loaded into memory. | **Duplicate:** Multiple independent block caches reading the same disk blocks. | **Efficient:** Single shared Ring-3 storage block cache, with compressed sub-byte index representations. |
| **6. Long-Term Extensibility** | **Rigid:** Adding a new index requires modifying the core unified storage engine schema. | **Fragile:** Format evolution requires upgrading multiple independent storage drivers. | **Unbounded:** New index services register as `EventProvider` subscribers without touching `gax_storage_engine` code. |

---

## 3. Recommended Architecture: Layered Storage Architecture

We recommend **Option C: Layered Storage Architecture**.

```
[Ring-3 Applications / User Space]
               │
               ▼
┌─────────────────────────────────────────────────────────────┐
│ Tier 2: Autonomous Provider-Based Services                  │
│  - vfs_server (NamespaceIndexProvider)                     │
│  - search_server (FullTextIndexProvider & SemanticProvider) │
│  - knowledge_server (GraphIndexProvider)                    │
└──────────────────────────────┬──────────────────────────────┘
                               │ (EventProvider Stream & Storage IPC)
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ Tier 1: Generic Storage Engine (gax_storage_engine)         │
│  - Dual Superblock Atomic Commit Generation                 │
│  - Copy-on-Write (CoW) Allocation & Page Management         │
│  - Cryptographic Integrity Verification (BLAKE3 v1 default) │
│  - Object Metadata Headers & Extent Allocation              │
│  - Private Storage Journal (gax_storage_journal)            │
└──────────────────────────────┬──────────────────────────────┘
                               │ (StorageDeviceProvider Capabilities)
                               ▼
[StorageDeviceProvider (virtio_block_server, nvme_server, NVDIMM)]
```

---

## 4. Architectural Decision Justification

Option C aligns directly with Gaxera's core design principles:
1. **Separation of Concerns:** `gax_storage_engine` owns transactional persistence, block allocation, recovery, and cryptographic integrity. Tier 2 provider services own specialized indexing and query optimization.
2. **Provider Independence:** Higher-level services interact solely with abstract `IndexProvider`, `EventProvider`, and `StorageDeviceProvider` interfaces.
3. **Event-Driven Synchronization:** Index services synchronize exclusively through `EventProvider` replay checkpoints. Zero hidden coupling exists.
4. **Capability Isolation:** Hard Ring-3 process memory boundaries prevent index bugs from corrupting primary storage pools.
5. **Future Storage Engine Replaceability:** `gax_storage_engine` remains fully replaceable beneath stable architectural interfaces (`GaxQL`, `query_planner`, `NamespaceProvider`, `CompatibilityProvider`, `CapabilityModel`, `EventProvider`).
