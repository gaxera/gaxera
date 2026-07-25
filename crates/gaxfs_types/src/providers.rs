//! GaxFS Abstract Provider Traits
//!
//! Provider interfaces insulating GaxFS architecture from physical storage,
//! indexing, compression, snapshot, and legacy OS compatibility backends.

use crate::capabilities::CapabilityHandle;
use crate::events::GaxFsEventRecord;
use crate::object_id::GaxObjectId;
use alloc::string::String;
use alloc::vec::Vec;

/// Storage device error types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum StorageError {
    IoError(String),
    OutOfBounds { requested: u64, max: u64 },
    ChecksumMismatch,
    DeviceNotReady,
}

/// Abstract storage device capability interface
pub trait StorageDeviceProvider: Send + Sync {
    fn read_blocks(
        &self,
        start_block: u64,
        num_blocks: u32,
        buf: &mut [u8],
    ) -> Result<(), StorageError>;
    fn write_blocks(
        &mut self,
        start_block: u64,
        num_blocks: u32,
        buf: &[u8],
    ) -> Result<(), StorageError>;
    fn flush_cache(&mut self) -> Result<(), StorageError>;
    fn block_size(&self) -> u32;
    fn total_blocks(&self) -> u64;
}

/// Entry in a namespace view projection
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NamespaceEntry {
    pub name: String,
    pub object_id: GaxObjectId,
    pub is_directory: bool,
}

/// Namespace provider error types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum NamespaceError {
    NotFound,
    AccessDenied,
    InvalidPath,
    AlreadyExists,
}

/// Abstract namespace provider interface
pub trait NamespaceProvider: Send + Sync {
    fn resolve_path(
        &self,
        scope: &CapabilityHandle,
        path: &str,
    ) -> Result<GaxObjectId, NamespaceError>;
    fn enumerate_view(
        &self,
        scope: &CapabilityHandle,
    ) -> Result<Vec<NamespaceEntry>, NamespaceError>;
}

/// Query Predicate AST expression placeholder for provider searches
#[derive(Debug, Clone, PartialEq)]
pub enum QueryPredicate {
    AttributeEquals {
        key: String,
        value: String,
    },
    SimilaritySearch {
        vector: Vec<f32>,
        top_k: usize,
    },
    RelationshipTarget {
        relationship: String,
        target: GaxObjectId,
    },
}

/// Index provider error types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum IndexError {
    IndexCorruption,
    UpdateFailed(String),
    QueryFailed(String),
}

/// Abstract IndexProvider supertrait interface
pub trait IndexProvider: Send + Sync {
    fn index_update(&mut self, record: &GaxFsEventRecord) -> Result<(), IndexError>;
    fn object_remove(&mut self, id: GaxObjectId) -> Result<(), IndexError>;
    fn query_execute(
        &self,
        predicate: &QueryPredicate,
        scope: &[GaxObjectId],
    ) -> Result<Vec<GaxObjectId>, IndexError>;
    fn event_replay(
        &mut self,
        from_sequence: u64,
        stream: &dyn EventProvider,
    ) -> Result<(), IndexError>;
    fn checkpoint_rebuild(&mut self, checkpoint_seq: u64) -> Result<(), IndexError>;
}

/// Event stream error types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum EventError {
    StreamClosed,
    SubscriberNotAuthorized,
    ReplayFailed(String),
}

/// Abstract EventProvider interface
pub trait EventProvider: Send + Sync {
    fn subscribe(&self, subscriber_cap: &CapabilityHandle) -> Result<u64, EventError>;
    fn publish(&mut self, record: &GaxFsEventRecord) -> Result<(), EventError>;
    fn replay(
        &self,
        from_sequence: u64,
        callback: &mut dyn FnMut(&GaxFsEventRecord) -> Result<(), EventError>,
    ) -> Result<u64, EventError>;
    fn checkpoint(&mut self, subscriber_id: u64, checkpoint_seq: u64) -> Result<(), EventError>;
}

/// Reported compression engine capabilities
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct CompressionCapabilities {
    pub supports_streaming: bool,
    pub supports_dictionary: bool,
    pub hardware_accelerated: bool,
    pub is_lossy: bool,
}

/// Compression provider error types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompressionError {
    CompressFailed(String),
    DecompressFailed(String),
    UnsupportedCapability,
}

/// Abstract CompressionProvider interface
pub trait CompressionProvider: Send + Sync {
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, CompressionError>;
    fn decompress(&self, compressed: &[u8]) -> Result<Vec<u8>, CompressionError>;
    fn capabilities(&self) -> CompressionCapabilities;
}

/// Snapshot error types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SnapshotError {
    NotFound,
    AccessDenied,
    CreationFailed(String),
    RollbackFailed(String),
}

/// Abstract SnapshotProvider interface
pub trait SnapshotProvider: Send + Sync {
    fn create_snapshot(
        &mut self,
        scope_handle: &CapabilityHandle,
        name: &str,
    ) -> Result<GaxObjectId, SnapshotError>;
    fn delete_snapshot(&mut self, snapshot_id: GaxObjectId) -> Result<(), SnapshotError>;
    fn rollback_snapshot(
        &mut self,
        target_scope: &CapabilityHandle,
        snapshot_id: GaxObjectId,
    ) -> Result<(), SnapshotError>;
    fn create_clone(
        &mut self,
        snapshot_id: GaxObjectId,
        new_name: &str,
    ) -> Result<(GaxObjectId, CapabilityHandle), SnapshotError>;
}

/// Legacy stat structure representation
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct LegacyStat {
    pub size: u64,
    pub mode: u32,
    pub modified_time: u64,
    pub is_dir: bool,
}

/// Compatibility provider error types
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompatibilityError {
    NotFound,
    AccessDenied,
    BadFileDescriptor,
    IoError(String),
}

/// Abstract CompatibilityProvider interface
pub trait CompatibilityProvider: Send + Sync {
    fn open(
        &self,
        scope: &CapabilityHandle,
        path: &str,
        flags: u32,
    ) -> Result<u64, CompatibilityError>;
    fn read(&self, handle: u64, buf: &mut [u8]) -> Result<usize, CompatibilityError>;
    fn write(&self, handle: u64, buf: &[u8]) -> Result<usize, CompatibilityError>;
    fn stat(&self, handle: u64) -> Result<LegacyStat, CompatibilityError>;
    fn close(&self, handle: u64) -> Result<(), CompatibilityError>;
}
