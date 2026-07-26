//! POSIX BSD Sockets Ring-3 Virtualization Layer.

use crate::net::NetSessionHandle;
use gaxera_abi::GaxObjectId;
use net_types::{IpAddr, Ipv4Addr, NetEndpoint, NetRights, ProviderError, SessionState};

pub const AF_INET: i32 = 2;
pub const SOCK_STREAM: i32 = 1;
pub const SOCK_DGRAM: i32 = 2;

/// Thread-safe BSD Virtual Descriptor Table mapping `int fd` to GaxNet capability handle.
pub struct BsdSocketTable {
    descriptors: [Option<NetSessionHandle>; 32],
}

impl Default for BsdSocketTable {
    fn default() -> Self {
        Self::new()
    }
}

impl BsdSocketTable {
    pub fn new() -> Self {
        Self {
            descriptors: [const { None }; 32],
        }
    }

    /// Virtual `socket(domain, type, protocol)` API.
    pub fn socket(
        &mut self,
        domain: i32,
        socket_type: i32,
        _protocol: i32,
    ) -> Result<i32, ProviderError> {
        if domain != AF_INET || (socket_type != SOCK_STREAM && socket_type != SOCK_DGRAM) {
            return Err(ProviderError::TransmissionFailed);
        }

        let dummy_endpoint = NetEndpoint::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0);
        let session = NetSessionHandle {
            id: GaxObjectId::generate(),
            rights: NetRights(NetRights::READ.0 | NetRights::WRITE.0 | NetRights::CONNECT.0),
            local_endpoint: dummy_endpoint,
            remote_endpoint: dummy_endpoint,
            state: SessionState::Created,
        };

        for (fd, slot) in self.descriptors.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(session);
                return Ok(fd as i32 + 3);
            }
        }
        Err(ProviderError::NotReady)
    }

    /// Virtual `connect(fd, addr, len)` API.
    pub fn connect(&mut self, fd: i32, remote: NetEndpoint) -> Result<(), ProviderError> {
        let idx = (fd - 3) as usize;
        if idx < self.descriptors.len() {
            if let Some(session) = &mut self.descriptors[idx] {
                if !session.rights.contains(NetRights::CONNECT) {
                    return Err(ProviderError::NotReady);
                }
                session.remote_endpoint = remote;
                session.state = SessionState::Established;
                return Ok(());
            }
        }
        Err(ProviderError::NotReady)
    }

    /// Virtual `close(fd)` API.
    pub fn close(&mut self, fd: i32) -> Result<(), ProviderError> {
        let idx = (fd - 3) as usize;
        if idx < self.descriptors.len() {
            self.descriptors[idx] = None;
            return Ok(());
        }
        Err(ProviderError::NotReady)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bsd_sockets_virtualization() {
        let mut table = BsdSocketTable::new();
        let fd = table.socket(AF_INET, SOCK_STREAM, 0).unwrap();
        assert_eq!(fd, 3);

        let remote = NetEndpoint::new(IpAddr::V4(Ipv4Addr::LOOPBACK), 8080);
        assert!(table.connect(fd, remote).is_ok());
        assert!(table.close(fd).is_ok());
    }
}
