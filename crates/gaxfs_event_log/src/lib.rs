//! GaxFS OS Event Log (`gaxfs_event_log`)
//!
//! Authoritative Ring-3 event publication engine implementing the `EventProvider` trait.
//! Provides zero-copy ring-buffer event publication, subscriber `EventCheckpointMarker` tracking,
//! log compaction, and deterministic index self-healing rehydration.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod checkpoint;
pub mod log;
pub mod ring_buffer;

pub use checkpoint::CheckpointTracker;
pub use log::GaxFsEventLog;
pub use ring_buffer::{DEFAULT_RING_CAPACITY, EventRingBuffer};
