//! IPv4/IPv6 IP Router and ICMP Echo Engine.

use net_types::{IcmpHeader, IpAddr, Ipv4Header, NetRoute};

pub struct IpRouter {
    pub routes: [Option<NetRoute>; 32],
}

impl Default for IpRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl IpRouter {
    pub fn new() -> Self {
        Self {
            routes: [const { None }; 32],
        }
    }

    pub fn add_route(&mut self, route: NetRoute) -> Result<(), net_types::ProviderError> {
        for slot in self.routes.iter_mut() {
            if slot.is_none() {
                *slot = Some(route);
                return Ok(());
            }
        }
        Err(net_types::ProviderError::NotReady)
    }

    pub fn lookup(&self, destination: IpAddr) -> Option<&NetRoute> {
        let mut best_match: Option<&NetRoute> = None;
        let mut max_prefix = 0u8;

        for route in self.routes.iter().flatten() {
            if route.destination_cidr.contains(&destination)
                && (best_match.is_none() || route.destination_cidr.prefix_len >= max_prefix)
            {
                max_prefix = route.destination_cidr.prefix_len;
                best_match = Some(route);
            }
        }

        best_match
    }

    pub fn process_icmp_packet(
        &self,
        _ip_hdr: &Ipv4Header,
        icmp_hdr: &IcmpHeader,
        payload: &[u8],
        out_buf: &mut [u8],
    ) -> Option<usize> {
        if icmp_hdr.icmp_type == IcmpHeader::TYPE_ECHO_REQUEST {
            let reply_hdr = IcmpHeader {
                icmp_type: IcmpHeader::TYPE_ECHO_REPLY,
                code: 0,
                checksum: icmp_hdr.checksum,
                rest_of_header: icmp_hdr.rest_of_header,
            };

            let total_len = 8 + payload.len();
            if out_buf.len() < total_len {
                return None;
            }

            out_buf[0] = reply_hdr.icmp_type;
            out_buf[1] = reply_hdr.code;
            out_buf[2..4].copy_from_slice(&reply_hdr.checksum.to_be_bytes());
            out_buf[4..8].copy_from_slice(&reply_hdr.rest_of_header);
            out_buf[8..total_len].copy_from_slice(payload);

            Some(total_len)
        } else {
            None
        }
    }
}
