//! Layered Provider Trait Contracts, Common Lifecycle, and Version Negotiation.

use crate::address::{IpAddr, MacAddress};
use crate::errors::ProviderError;
use crate::frame::FrameDescriptor;
use crate::headers::{EthernetHeader, IpHeaderSummary};
use crate::objects::{LinkStatus, NetEndpoint};
use alloc::vec::Vec;
use gaxera_abi::GaxObjectId;

/// Provider Common Lifecycle States.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum ProviderLifecycleState {
    Discovered = 0,
    Registered = 1,
    Initialized = 2,
    Ready = 3,
    Running = 4,
    Degraded = 5,
    Restarting = 6,
    Stopped = 7,
}

/// Provider IPC Version Negotiation Header (`ProviderIpcHeader`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct ProviderIpcHeader {
    pub protocol_magic: u32,   // Magic b"GAXN"
    pub protocol_version: u32, // IPC Version (1)
    pub message_type: u16,
    pub message_len: u32,
}

impl ProviderIpcHeader {
    pub const MAGIC: u32 = 0x4741584E; // b"GAXN"
    pub const CURRENT_VERSION: u32 = 1;

    pub fn new(message_type: u16, message_len: u32) -> Self {
        Self {
            protocol_magic: Self::MAGIC,
            protocol_version: Self::CURRENT_VERSION,
            message_type,
            message_len,
        }
    }

    pub fn verify_version(&self) -> Result<(), ProviderError> {
        if self.protocol_magic == Self::MAGIC && self.protocol_version == Self::CURRENT_VERSION {
            Ok(())
        } else {
            Err(ProviderError::VersionMismatch)
        }
    }
}

/// Layer 1: Hardware Device Provider Trait.
pub trait DeviceProvider: Send + Sync {
    fn mac_address(&self) -> MacAddress;
    fn mtu(&self) -> u32;
    fn link_status(&self) -> LinkStatus;
    fn transmit_frame(&self, descriptor: &FrameDescriptor) -> Result<(), ProviderError>;
    fn receive_frame(&self, descriptor: &mut FrameDescriptor) -> Result<(), ProviderError>;
}

/// Layer 2: Link Layer Provider Trait.
pub trait LinkProvider: Send + Sync {
    fn ethertype(&self) -> u16;
    fn parse_header<'a>(&self, packet: &'a [u8]) -> Option<(EthernetHeader, &'a [u8])>;
}

/// Layer 3: Network Routing Layer Provider Trait.
pub trait NetworkProvider: Send + Sync {
    fn route_lookup(&self, destination: IpAddr) -> Option<GaxObjectId>;
    fn process_packet<'a>(&self, packet: &'a [u8]) -> Option<(IpHeaderSummary, &'a [u8])>;
}

/// Layer 4: Transport Layer Protocol Provider Trait.
pub trait TransportProvider: Send + Sync {
    fn protocol_id(&self) -> u8; // 6 = TCP, 17 = UDP, 254 = QUIC
    fn create_session(
        &self,
        local: NetEndpoint,
        remote: NetEndpoint,
    ) -> Result<GaxObjectId, ProviderError>;
    fn close_session(&self, session_id: GaxObjectId) -> Result<(), ProviderError>;
}

/// Layer 5: Session Crypto Encryption Provider Trait.
pub trait CryptoProvider: Send + Sync {
    fn encrypt_payload(
        &self,
        plaintext: &[u8],
        ciphertext: &mut [u8],
    ) -> Result<usize, ProviderError>;
    fn decrypt_payload(
        &self,
        ciphertext: &[u8],
        plaintext: &mut [u8],
    ) -> Result<usize, ProviderError>;
}

/// Layer 6: Domain Name Resolver Provider Trait.
pub trait ResolverProvider: Send + Sync {
    fn resolve_domain(&self, domain: &str) -> Result<Vec<IpAddr>, ProviderError>;
}
