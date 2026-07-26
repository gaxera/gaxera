//! UDP Transport Layer Engine.

use alloc::vec::Vec;
use gaxera_abi::GaxObjectId;
use net_types::{NetEndpoint, ProviderError, TransportProvider};
use spinning_top::Spinlock;

#[derive(Clone, Debug)]
pub struct UdpSocketEntry {
    pub session_id: GaxObjectId,
    pub local_endpoint: NetEndpoint,
    pub remote_endpoint: NetEndpoint,
}

pub struct UdpTransportEngine {
    pub sockets: Spinlock<Vec<UdpSocketEntry>>,
}

impl Default for UdpTransportEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl UdpTransportEngine {
    pub fn new() -> Self {
        Self {
            sockets: Spinlock::new(Vec::new()),
        }
    }
}

impl TransportProvider for UdpTransportEngine {
    fn protocol_id(&self) -> u8 {
        17 // UDP
    }

    fn create_session(
        &self,
        local: NetEndpoint,
        remote: NetEndpoint,
    ) -> Result<GaxObjectId, ProviderError> {
        let session_id = GaxObjectId::generate();
        let entry = UdpSocketEntry {
            session_id,
            local_endpoint: local,
            remote_endpoint: remote,
        };
        self.sockets.lock().push(entry);
        Ok(session_id)
    }

    fn close_session(&self, session_id: GaxObjectId) -> Result<(), ProviderError> {
        let mut guard = self.sockets.lock();
        if let Some(pos) = guard.iter().position(|s| s.session_id == session_id) {
            guard.remove(pos);
            return Ok(());
        }
        Err(ProviderError::NotReady)
    }
}
