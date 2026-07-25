//! UDP Transport Layer Engine.

use gaxera_abi::GaxObjectId;
use net_types::{NetEndpoint, ProviderError, TransportProvider};

pub struct UdpSocketEntry {
    pub session_id: GaxObjectId,
    pub local_endpoint: NetEndpoint,
    pub remote_endpoint: NetEndpoint,
}

pub struct UdpTransportEngine {
    pub sockets: [Option<UdpSocketEntry>; 64],
}

impl Default for UdpTransportEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl UdpTransportEngine {
    pub fn new() -> Self {
        Self {
            sockets: [const { None }; 64],
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
        let _ = local;
        let _ = remote;
        Ok(session_id)
    }

    fn close_session(&self, _session_id: GaxObjectId) -> Result<(), ProviderError> {
        Ok(())
    }
}
