//! GaxNet Foundation Error Enums.

use core::fmt;

/// Header encoding and parsing errors.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum HeaderError {
    BufferTooShort = 1,
    InvalidField = 2,
    ChecksumMismatch = 3,
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Lifecycle state machine errors.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum LifecycleError {
    InvalidStateTransition = 1,
    SessionAlreadyClosed = 2,
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Provider execution and IPC errors.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u8)]
pub enum ProviderError {
    NotReady = 1,
    TransmissionFailed = 2,
    ReceptionFailed = 3,
    SessionCreationFailed = 4,
    CryptoError = 5,
    ResolverError = 6,
    VersionMismatch = 7,
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
