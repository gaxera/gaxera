//! GaxNet Ring-3 User-Space Protocol Server Library.

#![no_std]

pub mod arp;
pub mod ip_router;
pub mod namespace;
pub mod tcp;
pub mod udp;

pub use arp::ArpCache;
pub use ip_router::IpRouter;
pub use namespace::NetNamespaceManager;
pub use tcp::TcpTransportEngine;
pub use udp::UdpTransportEngine;

#[cfg(test)]
mod tests {
    use super::*;
    use net_types::{
        tcp_flags, ArpHeader, IcmpHeader, IpAddr, IpCidr, Ipv4Addr, Ipv4Header, MacAddress,
        NetEndpoint, NetRoute, SessionState, TcpHeader,
    };

    #[test]
    fn test_arp_cache_insert_lookup_and_reply() {
        let mut cache = ArpCache::new();
        let ip = Ipv4Addr::new(192, 168, 1, 100);
        let mac = MacAddress::new([0x00, 0x11, 0x22, 0x33, 0x44, 0x55]);

        cache.insert(ip, mac, 100);
        assert_eq!(cache.lookup(ip, 150), Some(mac));
        assert_eq!(cache.lookup(ip, 500), None); // Expired TTL

        let req = ArpHeader {
            htype: 1,
            ptype: 0x0800,
            hlen: 6,
            plen: 4,
            oper: ArpHeader::OPER_REQUEST,
            sender_mac: MacAddress::new([0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF]),
            sender_ip: Ipv4Addr::new(192, 168, 1, 1),
            target_mac: MacAddress::ZERO,
            target_ip: ip,
        };

        let reply = cache.process_arp_packet(&req, 200).unwrap();
        assert_eq!(reply.oper, ArpHeader::OPER_REPLY);
        assert_eq!(reply.target_ip, Ipv4Addr::new(192, 168, 1, 1));
    }

    #[test]
    fn test_ip_router_lookup_and_icmp_echo() {
        let mut router = IpRouter::new();
        let route = NetRoute {
            id: gaxera_abi::GaxObjectId::generate(),
            destination_cidr: IpCidr {
                ip: IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)),
                prefix_len: 8,
            },
            gateway: Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))),
            interface_id: gaxera_abi::GaxObjectId::generate(),
            metric: 10,
        };

        router.add_route(route).unwrap();
        let match_route = router
            .lookup(IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3)))
            .unwrap();
        assert_eq!(match_route.metric, 10);

        let ip_hdr = Ipv4Header {
            version_ihl: 0x45,
            dscp_ecn: 0,
            total_len: 28,
            identification: 1,
            flags_fragment: 0,
            ttl: 64,
            protocol: 1,
            checksum: 0,
            src_ip: Ipv4Addr::new(10, 0, 0, 2),
            dst_ip: Ipv4Addr::new(10, 0, 0, 1),
        };

        let icmp_req = IcmpHeader {
            icmp_type: IcmpHeader::TYPE_ECHO_REQUEST,
            code: 0,
            checksum: 0x1234,
            rest_of_header: [0, 1, 0, 1],
        };

        let mut out = [0u8; 64];
        let len = router
            .process_icmp_packet(&ip_hdr, &icmp_req, b"hello", &mut out)
            .unwrap();
        assert_eq!(len, 13);
        assert_eq!(out[0], IcmpHeader::TYPE_ECHO_REPLY);
    }

    #[test]
    fn test_tcp_stateful_handshake_and_flow_control() {
        let local = NetEndpoint::new(IpAddr::V4(Ipv4Addr::LOOPBACK), 80);
        let remote = NetEndpoint::new(IpAddr::V4(Ipv4Addr::LOOPBACK), 54321);

        let mut tcb = tcp::TcpControlBlock::new(local, remote);
        assert_eq!(tcb.state, SessionState::Created);

        let syn = TcpHeader {
            src_port: 54321,
            dst_port: 80,
            seq_num: 5000,
            ack_num: 0,
            data_offset_reserved: 0x50,
            flags: tcp_flags::SYN,
            window_size: 65535,
            checksum: 0,
            urgent_pointer: 0,
        };

        let syn_ack = tcb.process_segment(&syn, 0).unwrap();
        assert_eq!(tcb.state, SessionState::Established);
        assert_eq!(syn_ack.flags, tcp_flags::SYN | tcp_flags::ACK);
        assert_eq!(syn_ack.ack_num, 5001);
    }
}
