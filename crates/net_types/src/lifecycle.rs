//! Generic Transport-Independent Session Lifecycle and TransportInstance.

use crate::objects::NetEndpoint;
use gaxera_abi::GaxObjectId;

/// Transport-Independent Session Lifecycle States.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum SessionState {
    Created = 0,
    Connecting = 1,
    Established = 2,
    HalfClosed = 3,
    Closing = 4,
    Closed = 5,
    Destroyed = 6,
}

impl SessionState {
    /// Verify if a transition from `self` to `next` is allowed by the lifecycle state machine.
    pub fn can_transition_to(&self, next: Self) -> bool {
        match (*self, next) {
            (Self::Created, Self::Connecting) => true,
            (Self::Created, Self::Established) => true, // UDP immediate setup
            (Self::Connecting, Self::Established) => true,
            (Self::Connecting, Self::Closed) => true,
            (Self::Established, Self::HalfClosed) => true,
            (Self::Established, Self::Closing) => true,
            (Self::Established, Self::Closed) => true,
            (Self::HalfClosed, Self::Closing) => true,
            (Self::HalfClosed, Self::Closed) => true,
            (Self::Closing, Self::Closed) => true,
            (Self::Closed, Self::Destroyed) => true,
            _ => false,
        }
    }
}

/// Reasons for session closure or termination.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum CloseReason {
    CleanTeardown = 0,
    ConnectionReset = 1,
    ConnectionTimeout = 2,
    ProviderCrashed = 3,
    InterfaceUnavailable = 4,
    CapabilityRevoked = 5,
    LocalProcessExit = 6,
    CryptoProviderCrashed = 7,
}

/// Runtime Protocol State Wrapper (`TransportInstance`).
/// Owned by `TransportProvider`, encapsulating runtime protocol state.
#[derive(Clone, Debug)]
pub struct TransportInstance {
    pub session_id: GaxObjectId,
    pub protocol_id: u8, // 6 = TCP, 17 = UDP, 254 = QUIC
    pub local_endpoint: NetEndpoint,
    pub remote_endpoint: NetEndpoint,
    pub state: SessionState,
    pub close_reason: Option<CloseReason>,
}

impl TransportInstance {
    pub fn new(
        session_id: GaxObjectId,
        protocol_id: u8,
        local_endpoint: NetEndpoint,
        remote_endpoint: NetEndpoint,
    ) -> Self {
        Self {
            session_id,
            protocol_id,
            local_endpoint,
            remote_endpoint,
            state: SessionState::Created,
            close_reason: None,
        }
    }

    pub fn transition_to(
        &mut self,
        next: SessionState,
    ) -> Result<(), crate::errors::LifecycleError> {
        if self.state.can_transition_to(next) {
            self.state = next;
            Ok(())
        } else {
            Err(crate::errors::LifecycleError::InvalidStateTransition)
        }
    }
}
