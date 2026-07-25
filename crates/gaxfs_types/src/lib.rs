//! GaxFS Native Storage Platform Foundation Types & Trait Abstractions
//!
//! Authoritative, provider-independent type definitions and trait interfaces
//! for GaxFS objects, capability handles, event streams, index providers,
//! compression engines, snapshot managers, and compatibility layers.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod capabilities;
pub mod events;
pub mod object_id;
pub mod providers;

pub use capabilities::{CapabilityError, CapabilityHandle, CapabilitySpace, GaxFsRights};
pub use events::{GaxFsEventRecord, GaxFsEventType};
pub use object_id::GaxObjectId;
pub use providers::{
    CompatibilityError, CompatibilityProvider, CompressionCapabilities, CompressionError,
    CompressionProvider, EventError, EventProvider, IndexError, IndexProvider, LegacyStat,
    NamespaceEntry, NamespaceError, NamespaceProvider, QueryPredicate, SnapshotError,
    SnapshotProvider, StorageDeviceProvider, StorageError,
};
