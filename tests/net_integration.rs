//! End-to-End GaxNet Integration Test Suite.

use crypto_server::TlsCryptoServer;
use libgaxera::compat::sockets::{BsdSocketTable, AF_INET, SOCK_STREAM};
use net_stack_server::{ArpCache, IpRouter, TcpTransportEngine};
use net_types::{
    BackpressurePolicy, CryptoProvider, DeviceProvider, IpAddr, IpCidr, Ipv4Addr, LinkStatus,
    MacAddress, NetEndpoint, NetRights, NetRoute, PacketRingHeader, ProviderIpcHeader,
    ResolverProvider, RingType, TransportProvider,
};
use resolver_server::DnsResolverServer;
use virtio_net_server::VirtioNetDriver;

#[test]
fn test_end_to_end_gaxnet_stack_pipeline() {
    // 1. Driver Layer
    let mac = MacAddress::new([0x52, 0x54, 0x00, 0xAB, 0xCD, 0xEF]);
    let driver = VirtioNetDriver::new(mac);
    assert_eq!(driver.mac_address(), mac);
    assert_eq!(driver.link_status(), LinkStatus::Up);

    // 2. Shared Memory Ring Buffer & Backpressure
    let ring = PacketRingHeader::new(128, RingType::Tx);
    assert_eq!(ring.ring_type, RingType::Tx as u8);
    assert_eq!(ring.policy, BackpressurePolicy::BlockProducer as u8);
    let slot = ring.push_slot().unwrap();
    assert_eq!(slot, 0);

    // 3. ARP Resolution Cache
    let mut arp_cache = ArpCache::new();
    let peer_ip = Ipv4Addr::new(10, 0, 0, 2);
    let peer_mac = MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);
    arp_cache.insert(peer_ip, peer_mac, 1000);
    assert_eq!(arp_cache.lookup(peer_ip, 1050), Some(peer_mac));

    // 4. IP Routing
    let mut router = IpRouter::new();
    let route = NetRoute {
        id: gaxera_abi::GaxObjectId::generate(),
        destination_cidr: IpCidr {
            ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)),
            prefix_len: 24,
        },
        gateway: Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
        interface_id: gaxera_abi::GaxObjectId::generate(),
        metric: 5,
    };
    router.add_route(route).unwrap();
    assert!(router.lookup(IpAddr::V4(peer_ip)).is_some());

    // 5. Stateful TCP Connection & Session Lifecycle
    let tcp_engine = TcpTransportEngine::new();
    let local_ep = NetEndpoint::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 8080);
    let remote_ep = NetEndpoint::new(IpAddr::V4(peer_ip), 51234);
    let session_id = tcp_engine.create_session(local_ep, remote_ep).unwrap();
    assert!(!session_id.as_bytes().iter().all(|&b| b == 0));

    // 6. Capability Security & Attenuation
    let root_rights = NetRights(
        NetRights::READ.0 | NetRights::WRITE.0 | NetRights::CONNECT.0 | NetRights::LISTEN.0,
    );
    let desired_rights = NetRights(NetRights::READ.0 | NetRights::WRITE.0);
    let child_rights = root_rights.derive_narrowed(desired_rights);
    assert!(child_rights.contains(NetRights::READ));
    assert!(child_rights.contains(NetRights::WRITE));
    assert!(!child_rights.contains(NetRights::LISTEN));

    // 7. Domain Resolver Service
    let resolver = DnsResolverServer;
    let addrs = resolver.resolve_domain("localhost").unwrap();
    assert_eq!(addrs[0], IpAddr::V4(Ipv4Addr::LOOPBACK));

    // 8. Session Crypto Service
    let crypto = TlsCryptoServer;
    let payload = b"GaxNet Secure Data Payload";
    let mut cipher = [0u8; 64];
    let mut decrypted = [0u8; 64];
    let enc_len = crypto.encrypt_payload(payload, &mut cipher).unwrap();
    let dec_len = crypto
        .decrypt_payload(&cipher[..enc_len], &mut decrypted)
        .unwrap();
    assert_eq!(&decrypted[..dec_len], payload);

    // 9. POSIX BSD Socket Virtualization Wrapper
    let mut bsd_table = BsdSocketTable::new();
    let fd = bsd_table.socket(AF_INET, SOCK_STREAM, 0).unwrap();
    assert!(fd >= 3);
    assert!(bsd_table.connect(fd, remote_ep).is_ok());
    assert!(bsd_table.close(fd).is_ok());

    // 10. Provider Version Negotiation
    let ipc_hdr = ProviderIpcHeader::new(1, 256);
    assert!(ipc_hdr.verify_version().is_ok());
}
