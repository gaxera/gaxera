# GaxFS Indexing & Three-Layer Query Architecture Specification

> **Status:** Authoritative Architecture Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `search_server`, `knowledge_server`, `query_planner`  

---

## 1. Three-Layer Query Architecture

Query processing in Gaxera is structured into three strictly decoupled layers:

```
[Layer 3: Language-Native Bindings] ── (Rust, C, C++, Python, Zig Builders)
                 │
                 ▼ (Constructs Abstract Syntax Tree)
[Layer 1: GaxQL (Platform-Independent Declarative AST)]
                 │
                 ▼ (Parses AST & Optimizes Plan)
[Layer 2: Query Planner (query_planner)]
                 │
  ┌──────────────┼──────────────┬──────────────┬──────────────┬──────────────┐
  ▼              ▼              ▼              ▼              ▼              ▼
[Namespace]  [Metadata]    [FullText]     [Semantic]      [Graph]       [Activity]
 (Provider)  (Provider)    (Provider)     (Provider)    (Provider)     (Provider)
```

### 1.1 `query_planner` Ownership Boundaries
`query_planner` is a Ring-3 service that owns query planning, cost optimization, provider selection, execution orchestration, candidate result merging, and capability authorization filtering.
- **DOES NOT OWN:** Physical storage, index internal structures, metadata dictionaries, or namespace management.

---

## 2. Core Architectural Invariants for Querying

> **Invariant 1 — Queries Describe Intent, Never Execution:**  
> A GaxQL query contains zero execution ordering hints, zero algorithm assumptions, zero index hints, and zero provider names. Applications describe desired results; `query_planner` determines execution strategy.
> 
> **Invariant 2 — Capability Security & Zero Information Leakage:**  
> Capability enforcement occurs **before** query results are exposed to applications. No `IndexProvider` may leak unauthorized object identities, metadata attributes, similarity rankings, or existence information outside the caller's authorized `CapabilitySpace`.
> 
> **Invariant 3 — Domain Operations vs. Implementation Abstraction:**  
> Queries may use domain-level semantic operations (`SimilarTo`, `References`, `DependsOn`, `Contains`, `RelatedTo`). Implementation-level names (`TurboVec`, `HNSW`, `DiskANN`, `BTree`) are strictly prohibited in queries and ASTs.
> 
> **Invariant 4 — Event-Driven Index Synchronization:**  
> `IndexProvider` implementations synchronize exclusively through `GaxFsEventLog` replay checkpoints; zero direct filesystem coupling or hidden synchronization channels exist.

---

## 3. Generic `IndexProvider` Architecture

All indexing engines implement a common architectural supertrait: **`IndexProvider`**.

```rust
pub trait IndexProvider {
    /// Applies an incremental object update to the index
    fn index_update(&mut self, record: &GaxFsEventRecord) -> Result<(), IndexError>;
    
    /// Removes an object from the index upon deletion
    fn object_remove(&mut self, id: GaxObjectId) -> Result<(), IndexError>;
    
    /// Executes a provider-specific query predicate
    fn query_execute(&self, predicate: &QueryPredicate, scope: &[GaxObjectId]) -> Result<Vec<GaxObjectId>, IndexError>;
    
    /// Replays events from a given sequence checkpoint to rehydrate state
    fn event_replay(&mut self, from_sequence: u64, journal: &GaxFsEventLog) -> Result<(), IndexError>;
    
    /// Rebuilds index state from an authoritative checkpoint marker
    fn checkpoint_rebuild(&mut self, checkpoint: &EventCheckpointMarker) -> Result<(), IndexError>;
}
```

### 3.1 Specialized `IndexProvider` Domains
`query_planner` interacts strictly with eight abstract provider interfaces:
1. **`NamespaceIndexProvider`:** Path resolution, directory tree projections, capability mount points.
2. **`MetadataIndexProvider`:** Attribute filter evaluation (`author`, `tags`, custom metadata).
3. **`FullTextIndexProvider`:** Searchable textual object content indexing.
4. **`SemanticIndexProvider`:** Semantic similarity over vector-representable object attributes.
5. **`GraphIndexProvider`:** Directed object relationship links (`GeneratedFrom`, `DependsOn`, `References`).
6. **`ActivityIndexProvider`:** Recency, frequency, access history, pinned favorites.
7. **`PolicyIndexProvider`:** Capability delegation paths, sharing rules, access audit logs.
8. **`IntegrityIndexProvider`:** Checksum verification state, bitrot tracking, remote replica sync.

---

## 4. Generic `CompressionProvider` Framework

Compression is abstracted behind a generic `CompressionProvider` trait supporting conceptual capabilities:

```rust
pub trait CompressionProvider {
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, CompressionError>;
    fn decompress(&self, compressed: &[u8]) -> Result<Vec<u8>, CompressionError>;
    
    /// Reports provider compression capabilities (streaming, dictionary, hardware accelerated)
    fn capabilities(&self) -> CompressionCapabilities;
}
```

- **Pluggable Providers:** `TurboQuantProvider` (lossy numerical/vector quantization), `ZstdProvider` (lossless high-ratio payload compression), `Lz4Provider` (streaming memory compression).
