//! Native GaxNet Client Abstractions.

use gaxera_abi::GaxObjectId;
use net_types::{FrameDescriptor, NetEndpoint, NetRights, ProviderError, SessionState};

/// Native Active Session Capability Handle (`NetSessionHandle`).
#[derive(Clone, Debug)]
pub struct NetSessionHandle {
    pub id: GaxObjectId,
    pub rights: NetRights,
    pub local_endpoint: NetEndpoint,
    pub remote_endpoint: NetEndpoint,
    pub state: SessionState,
}

impl NetSessionHandle {
    pub fn new(
        id: GaxObjectId,
        rights: NetRights,
        local: NetEndpoint,
        remote: NetEndpoint,
    ) -> Self {
        Self {
            id,
            rights,
            local_endpoint: local,
            remote_endpoint: remote,
            state: SessionState::Established,
        }
    }

    pub fn send_frame(&self, frame: &FrameDescriptor) -> Result<(), ProviderError> {
        if !self.rights.contains(NetRights::WRITE) {
            return Err(ProviderError::NotReady);
        }
        if frame.payload_len == 0 {
            return Err(ProviderError::TransmissionFailed);
        }
        Ok(())
    }

    pub fn receive_frame(&self, frame: &mut FrameDescriptor) -> Result<(), ProviderError> {
        if !self.rights.contains(NetRights::READ) {
            return Err(ProviderError::NotReady);
        }
        frame.flags = 0x01; // Frame Ready sentinel
        Ok(())
    }
}

/// Native Listening Port Capability Handle (`NetListenerHandle`).
#[derive(Clone, Debug)]
pub struct NetListenerHandle {
    pub id: GaxObjectId,
    pub rights: NetRights,
    pub bind_endpoint: NetEndpoint,
}

impl NetListenerHandle {
    pub fn new(id: GaxObjectId, rights: NetRights, bind_endpoint: NetEndpoint) -> Self {
        Self {
            id,
            rights,
            bind_endpoint,
        }
    }

    pub fn accept(&self) -> Result<NetSessionHandle, ProviderError> {
        if !self.rights.contains(NetRights::LISTEN) {
            return Err(ProviderError::NotReady);
        }
        let session_id = GaxObjectId::generate();
        Ok(NetSessionHandle::new(
            session_id,
            NetRights(NetRights::READ.0 | NetRights::WRITE.0 | NetRights::CONTROL.0),
            self.bind_endpoint,
            self.bind_endpoint,
        ))
    }
}
