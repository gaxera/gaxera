# GaxFS Event Model & Storage Journal Specification

> **Status:** Authoritative Architecture Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `search_server`, `knowledge_server`, `query_planner`  

---

## 1. Architectural Separation: Storage Journal vs. Public Event Stream

GaxFS makes a strict conceptual separation between **Internal Storage Journaling** and **Public OS Event Streaming**:

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Filesystem Journal (gax_storage_journal)                 │
│    - Owned 100% privately by gax_storage_engine             │
│    - Responsibilities: Transactional durability, dual       │
│      superblock commits, storage block consistency.         │
└──────────────────────────────┬──────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│ 2. Public OS Event Stream (EventStream / GaxFsEventLog)     │
│    - Core Architectural Abstraction & OS Event Publication  │
│    - Responsibilities: Index rebuilding, event publication, │
│      snapshot markers, synchronization, audit, telemetry.   │
└─────────────────────────────────────────────────────────────┘
```

---

## 2. Generic `EventProvider` Architecture

The event system is abstracted behind a generic architectural contract: **`EventProvider`**.

```rust
pub trait EventProvider {
    /// Subscribes an authorized Ring-3 service to the event stream
    fn subscribe(&self, subscriber_cap: &CapabilityHandle) -> Result<SubscriptionHandle, EventError>;
    
    /// Publishes a committed event record to the persistent stream
    fn publish(&mut self, record: &GaxFsEventRecord) -> Result<(), EventError>;
    
    /// Replays event records from a starting sequence ID
    fn replay(&self, from_sequence: u64, consumer: &mut impl EventConsumer) -> Result<u64, EventError>;
    
    /// Records a subscriber state checkpoint for log compaction
    fn checkpoint(&mut self, subscriber_id: u64, checkpoint_seq: u64) -> Result<(), EventError>;
}
```

### Provider Implementations:
- **`GaxFsEventLog` (Default v1 Implementation):** Authoritative, zero-copy, persistent Ring-3 event log.
- **Pluggable Future Providers:** Distributed event providers, replicated event streams, remote sync providers, testing mocks.

---

## 3. Generalised Event Consumers & Subscriber Ownership

Any authorized Ring-3 service (e.g. `search_server`, `knowledge_server`, `backup_server`, telemetry daemons, custom application plugins) may subscribe through `EventProvider`.

```
[GaxFS Engine] ──(Publish)──> [EventProvider (GaxFsEventLog)]
                                        │
        ┌───────────────────────────────┼───────────────────────────────┐
        ▼                               ▼                               ▼
[search_server]                 [knowledge_server]              [Ring-3 Consumer]
(Subscriber-Owned Replay)       (Subscriber-Owned Replay)       (Subscriber-Owned Replay)
```

### Subscriber Ownership Responsibilities:
- Subscribers own their **replay position**, **local index checkpoints**, and **recovery state**.
- `EventProvider` provides the authoritative persistent event history.

---

## 4. Core Invariants & Ordering Guarantees

> **Invariant 1 — Single Authoritative Replay Synchronization:**  
> Every subsystem that derives secondary state from GaxFS (search indexes, graph indexes, backup state, telemetry) **MUST rebuild exclusively from authoritative event replay**. Zero hidden side channels exist.
> 
> **Invariant 2 — Provider Independence:**  
> Applications and services depend on abstract `EventProvider` capabilities rather than concrete event log structures.
> 
> **Ordering Guarantees:**  
> 1. Events within a committed transaction are strictly ordered by sequence number.
> 2. Committed transactions are replayed in monotonic commit generation sequence order.

---

## 5. Event Payload Definitions & Evolution

```rust
#[repr(u16)]
pub enum GaxFsEventType {
    ObjectCreated = 1,
    ObjectModified = 2,
    ObjectDeleted = 3,
    MetadataChanged = 4,
    RelationshipChanged = 5,
    SnapshotCreated = 6,
    SnapshotDeleted = 7,
    CapabilityChanged = 8,
    EventCheckpointMarker = 9,
    CustomExtension = 10,
}

#[repr(C)]
pub struct GaxFsEventRecord {
    pub sequence_id: u64,           // Monotonic event sequence number
    pub timestamp: u64,             // Commit timestamp
    pub event_type: GaxFsEventType, // Event type discriminator
    pub target_object: GaxObjectId, // Affected object ID
    pub owner_domain: u32,          // Resource domain ID
    pub extent_delta_blocks: u32,   // Number of modified blocks
    pub checksum: [u8; 32],         // Cryptographic integrity checksum (BLAKE3 v1 default)
    pub payload_len: u32,           // Versioned payload extension length
}
```

---

## 6. Event Replay, Log Compaction & Self-Healing

1. **Replay Checkpoints (`EventCheckpointMarker`):** Subscribers periodically write `EventCheckpointMarker { sequence_id }` back to `EventProvider`.
2. **Log Compaction:** `EventProvider` safely prunes stream storage prior to the lowest active checkpoint sequence number.
3. **Deterministic Rebuild:** If a subscriber crashes or suffers state corruption, it re-reads its last valid `EventCheckpointMarker` and replays log records from `sequence_id + 1` forward to achieve deterministic self-healing.
