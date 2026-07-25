//! GaxNet Core Foundation Types, Capability Security, and Descriptor Abstractions.

#![no_std]

extern crate alloc;

pub mod address;
pub mod errors;
pub mod frame;
pub mod headers;
pub mod lifecycle;
pub mod objects;
pub mod providers;
pub mod rights;
pub mod ring;

pub use address::{IpAddr, IpCidr, Ipv4Addr, Ipv6Addr, MacAddress};
pub use errors::{HeaderError, LifecycleError, ProviderError};
pub use frame::{frame_flags, FrameDescriptor, FrameType};
pub use headers::{
    ethertype, ip_protocol, tcp_flags, ArpHeader, EthernetHeader, IcmpHeader, Ipv4Header,
    TcpHeader, UdpHeader,
};
pub use lifecycle::{CloseReason, SessionState, TransportInstance};
pub use objects::{GaxNetObject, LinkStatus, NetEndpoint, NetInterface, NetRoute, ProtocolFamily};
pub use providers::{
    CryptoProvider, DeviceProvider, LinkProvider, NetworkProvider, ProviderIpcHeader,
    ProviderLifecycleState, ResolverProvider, TransportProvider,
};
pub use rights::{DomainPattern, NetCapabilityPolicy, NetRights};
pub use ring::{BackpressurePolicy, PacketRingHeader, RingType};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mac_address_formatting_and_checks() {
        let mac = MacAddress::new([0x00, 0x1A, 0x2B, 0x3C, 0x4D, 0x5E]);
        assert!(!mac.is_broadcast());
        assert!(!mac.is_multicast());

        let bcast = MacAddress::BROADCAST;
        assert!(bcast.is_broadcast());

        let mcast = MacAddress::new([0x01, 0x00, 0x5E, 0x00, 0x00, 0x01]);
        assert!(mcast.is_multicast());
    }

    #[test]
    fn test_ipv4_cidr_matching() {
        let cidr = IpCidr {
            ip: IpAddr::V4(Ipv4Addr::new(192, 168, 1, 0)),
            prefix_len: 24,
        };

        let target1 = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 50));
        let target2 = IpAddr::V4(Ipv4Addr::new(192, 168, 2, 1));

        assert!(cidr.contains(&target1));
        assert!(!cidr.contains(&target2));
    }

    #[test]
    fn test_net_rights_attenuation() {
        let parent = NetRights(NetRights::READ.0 | NetRights::WRITE.0 | NetRights::CONNECT.0);
        let desired = NetRights(NetRights::READ.0 | NetRights::LISTEN.0);

        let child = parent.derive_narrowed(desired);
        assert!(child.contains(NetRights::READ));
        assert!(!child.contains(NetRights::WRITE));
        assert!(!child.contains(NetRights::LISTEN));
    }

    #[test]
    fn test_packet_ring_header_spsc_math() {
        let ring = PacketRingHeader::new(64, RingType::Tx);
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);

        let slot1 = ring.push_slot().unwrap();
        assert_eq!(slot1, 0);
        assert_eq!(ring.len(), 1);

        let slot2 = ring.push_slot().unwrap();
        assert_eq!(slot2, 1);
        assert_eq!(ring.len(), 2);

        let popped1 = ring.pop_slot().unwrap();
        assert_eq!(popped1, 0);
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn test_ethernet_header_encode_parse() {
        let header = EthernetHeader {
            dst_mac: MacAddress::BROADCAST,
            src_mac: MacAddress::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x01]),
            ethertype: ethertype::IPV4,
        };

        let mut buf = [0u8; 14];
        header.encode(&mut buf).unwrap();

        let (parsed, payload) = EthernetHeader::parse(&buf).unwrap();
        assert_eq!(parsed, header);
        assert_eq!(payload.len(), 0);
    }

    #[test]
    fn test_session_state_machine() {
        let mut inst = TransportInstance::new(
            gaxera_abi::GaxObjectId::generate(),
            6,
            NetEndpoint::new(IpAddr::V4(Ipv4Addr::LOOPBACK), 8080),
            NetEndpoint::new(IpAddr::V4(Ipv4Addr::LOOPBACK), 9090),
        );

        assert_eq!(inst.state, SessionState::Created);
        assert!(inst.transition_to(SessionState::Connecting).is_ok());
        assert!(inst.transition_to(SessionState::Established).is_ok());
        assert!(inst.transition_to(SessionState::HalfClosed).is_ok());
        assert!(inst.transition_to(SessionState::Closed).is_ok());
        assert!(inst.transition_to(SessionState::Destroyed).is_ok());
    }

    #[test]
    fn test_provider_ipc_version_negotiation() {
        let header = ProviderIpcHeader::new(1, 100);
        assert!(header.verify_version().is_ok());

        let invalid = ProviderIpcHeader {
            protocol_magic: 0x12345678,
            protocol_version: 99,
            message_type: 1,
            message_len: 100,
        };
        assert!(invalid.verify_version().is_err());
    }
}
