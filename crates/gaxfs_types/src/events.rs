//! GaxFS Event Record & Event Stream Definitions
//!
//! Event payload schemas for public GaxFsEventLog streams.

use crate::object_id::GaxObjectId;

/// Event Type Discriminator
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
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

/// Canonical GaxFS Event Record structure.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct GaxFsEventRecord {
    pub sequence_id: u64,
    pub timestamp: u64,
    pub event_type: GaxFsEventType,
    pub target_object: GaxObjectId,
    pub owner_domain: u32,
    pub extent_delta_blocks: u32,
    pub checksum: [u8; 32],
    pub payload_len: u32,
}

impl GaxFsEventRecord {
    /// Creates a new event record
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sequence_id: u64,
        timestamp: u64,
        event_type: GaxFsEventType,
        target_object: GaxObjectId,
        owner_domain: u32,
        extent_delta_blocks: u32,
        checksum: [u8; 32],
        payload_len: u32,
    ) -> Self {
        Self {
            sequence_id,
            timestamp,
            event_type,
            target_object,
            owner_domain,
            extent_delta_blocks,
            checksum,
            payload_len,
        }
    }
}
