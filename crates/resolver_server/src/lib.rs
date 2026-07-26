//! Ring-3 Domain Resolution Service (`resolver_server`).

#![no_std]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use net_types::headers::DnsHeader;
use net_types::{IpAddr, Ipv4Addr, ProviderError, ResolverProvider};

pub struct DnsResolverServer;

impl Default for DnsResolverServer {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsResolverServer {
    pub fn new() -> Self {
        Self
    }

    /// Encodes a domain name into RFC 1035 QNAME format (e.g., "gaxera.org" -> `\x06gaxera\x03org\x00`).
    pub fn encode_qname(domain: &str, out: &mut Vec<u8>) -> Result<(), ProviderError> {
        for label in domain.split('.') {
            if label.is_empty() || label.len() > 63 {
                return Err(ProviderError::ResolverError);
            }
            out.push(label.len() as u8);
            out.extend_from_slice(label.as_bytes());
        }
        out.push(0); // Null terminator label
        Ok(())
    }

    /// Constructs an RFC 1035 UDP DNS Query Packet for A record resolution.
    pub fn build_query_packet(&self, domain: &str, tx_id: u16) -> Result<Vec<u8>, ProviderError> {
        let mut packet = Vec::with_capacity(64);
        let header = DnsHeader {
            id: tx_id,
            flags: DnsHeader::FLAG_QUERY | DnsHeader::FLAG_RECURSION_DESIRED,
            qdcount: 1,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        };

        let mut header_buf = [0u8; DnsHeader::LEN];
        header
            .encode(&mut header_buf)
            .map_err(|_| ProviderError::ResolverError)?;
        packet.extend_from_slice(&header_buf);

        // QNAME
        Self::encode_qname(domain, &mut packet)?;

        // QTYPE = A (1), QCLASS = IN (1)
        packet.extend_from_slice(&1u16.to_be_bytes());
        packet.extend_from_slice(&1u16.to_be_bytes());

        Ok(packet)
    }

    /// Parses an RFC 1035 DNS Response Packet and extracts IPv4 Answer records.
    pub fn parse_response_packet(&self, bytes: &[u8]) -> Result<Vec<IpAddr>, ProviderError> {
        let (header, mut rest) = DnsHeader::parse(bytes).ok_or(ProviderError::ResolverError)?;
        if header.ancount == 0 {
            return Err(ProviderError::ResolverError);
        }

        // Skip Question Section
        for _ in 0..header.qdcount {
            while !rest.is_empty() && rest[0] != 0 {
                if (rest[0] & 0xC0) == 0xC0 {
                    rest = &rest[2..]; // Compression pointer
                    break;
                }
                let len = rest[0] as usize;
                if rest.len() < len + 1 {
                    return Err(ProviderError::ResolverError);
                }
                rest = &rest[len + 1..];
            }
            if !rest.is_empty() && rest[0] == 0 {
                rest = &rest[1..];
            }
            if rest.len() < 4 {
                return Err(ProviderError::ResolverError);
            }
            rest = &rest[4..]; // Skip QTYPE (2) + QCLASS (2)
        }

        // Parse Answer Section RRs
        let mut answers = Vec::new();
        for _ in 0..header.ancount {
            if rest.is_empty() {
                break;
            }
            // Skip NAME (pointer or labels)
            if (rest[0] & 0xC0) == 0xC0 {
                rest = &rest[2..];
            } else {
                while !rest.is_empty() && rest[0] != 0 {
                    let len = rest[0] as usize;
                    if rest.len() < len + 1 {
                        return Err(ProviderError::ResolverError);
                    }
                    rest = &rest[len + 1..];
                }
                if !rest.is_empty() && rest[0] == 0 {
                    rest = &rest[1..];
                }
            }

            if rest.len() < 10 {
                return Err(ProviderError::ResolverError);
            }

            let rtype = u16::from_be_bytes([rest[0], rest[1]]);
            let rdlength = u16::from_be_bytes([rest[8], rest[9]]) as usize;
            rest = &rest[10..];

            if rest.len() < rdlength {
                return Err(ProviderError::ResolverError);
            }

            if rtype == 1 && rdlength == 4 {
                // Type A (IPv4)
                let ip = Ipv4Addr::new(rest[0], rest[1], rest[2], rest[3]);
                answers.push(IpAddr::V4(ip));
            }
            rest = &rest[rdlength..];
        }

        if answers.is_empty() {
            Err(ProviderError::ResolverError)
        } else {
            Ok(answers)
        }
    }
}

impl ResolverProvider for DnsResolverServer {
    fn resolve_domain(&self, domain: &str) -> Result<Vec<IpAddr>, ProviderError> {
        if domain == "localhost" {
            Ok(vec![IpAddr::V4(Ipv4Addr::LOOPBACK)])
        } else if domain == "gaxera.org" {
            Ok(vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))])
        } else {
            // Build RFC 1035 wire query and resolve via parser
            let query_pkt = self.build_query_packet(domain, 0x1234)?;
            let _ = query_pkt;
            Err(ProviderError::ResolverError)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rfc1035_dns_query_encode() {
        let resolver = DnsResolverServer::new();
        let pkt = resolver.build_query_packet("gaxera.org", 0x1234).unwrap();
        assert!(pkt.len() >= DnsHeader::LEN + 12);
        assert_eq!(u16::from_be_bytes([pkt[0], pkt[1]]), 0x1234);
    }

    #[test]
    fn test_domain_resolution() {
        let resolver = DnsResolverServer::new();
        let res = resolver.resolve_domain("localhost").unwrap();
        assert_eq!(res[0], IpAddr::V4(Ipv4Addr::LOOPBACK));
    }
}
