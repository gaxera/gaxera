//! Zero-allocation Packet Header Encoding & Decoding.

use crate::address::{Ipv4Addr, MacAddress};

/// EtherType Constants.
pub mod ethertype {
    pub const IPV4: u16 = 0x0800;
    pub const ARP: u16 = 0x0806;
    pub const IPV6: u16 = 0x86DD;
}

/// IP Protocol Numbers.
pub mod ip_protocol {
    pub const ICMP: u8 = 1;
    pub const TCP: u8 = 6;
    pub const UDP: u8 = 17;
}

/// Ethernet II Header (14 bytes).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(C, packed)]
pub struct EthernetHeader {
    pub dst_mac: MacAddress,
    pub src_mac: MacAddress,
    pub ethertype: u16, // Big-endian
}

impl EthernetHeader {
    pub const LEN: usize = 14;

    pub fn parse(bytes: &[u8]) -> Option<(Self, &[u8])> {
        if bytes.len() < Self::LEN {
            return None;
        }
        let mut dst = [0u8; 6];
        let mut src = [0u8; 6];
        dst.copy_from_slice(&bytes[0..6]);
        src.copy_from_slice(&bytes[6..12]);
        let ethertype = u16::from_be_bytes([bytes[12], bytes[13]]);
        Some((
            Self {
                dst_mac: MacAddress::new(dst),
                src_mac: MacAddress::new(src),
                ethertype,
            },
            &bytes[Self::LEN..],
        ))
    }

    pub fn encode(&self, out: &mut [u8]) -> Result<usize, crate::errors::HeaderError> {
        if out.len() < Self::LEN {
            return Err(crate::errors::HeaderError::BufferTooShort);
        }
        out[0..6].copy_from_slice(&self.dst_mac.0);
        out[6..12].copy_from_slice(&self.src_mac.0);
        out[12..14].copy_from_slice(&self.ethertype.to_be_bytes());
        Ok(Self::LEN)
    }
}

/// ARP Header (28 bytes for IPv4 over Ethernet).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ArpHeader {
    pub htype: u16, // 1 = Ethernet
    pub ptype: u16, // 0x0800 = IPv4
    pub hlen: u8,   // 6
    pub plen: u8,   // 4
    pub oper: u16,  // 1 = Request, 2 = Reply
    pub sender_mac: MacAddress,
    pub sender_ip: Ipv4Addr,
    pub target_mac: MacAddress,
    pub target_ip: Ipv4Addr,
}

impl ArpHeader {
    pub const LEN: usize = 28;

    pub const OPER_REQUEST: u16 = 1;
    pub const OPER_REPLY: u16 = 2;

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::LEN {
            return None;
        }
        let htype = u16::from_be_bytes([bytes[0], bytes[1]]);
        let ptype = u16::from_be_bytes([bytes[2], bytes[3]]);
        let hlen = bytes[4];
        let plen = bytes[5];
        let oper = u16::from_be_bytes([bytes[6], bytes[7]]);

        let mut smac = [0u8; 6];
        smac.copy_from_slice(&bytes[8..14]);
        let sip = Ipv4Addr::new(bytes[14], bytes[15], bytes[16], bytes[17]);

        let mut tmac = [0u8; 6];
        tmac.copy_from_slice(&bytes[18..24]);
        let tip = Ipv4Addr::new(bytes[24], bytes[25], bytes[26], bytes[27]);

        Some(Self {
            htype,
            ptype,
            hlen,
            plen,
            oper,
            sender_mac: MacAddress::new(smac),
            sender_ip: sip,
            target_mac: MacAddress::new(tmac),
            target_ip: tip,
        })
    }
}

/// Summary of parsed IP Header information.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct IpHeaderSummary {
    pub src_ip: crate::address::IpAddr,
    pub dst_ip: crate::address::IpAddr,
    pub protocol: u8,
    pub payload_len: u16,
}

/// IPv4 Packet Header (20 bytes min).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Ipv4Header {
    pub version_ihl: u8,
    pub dscp_ecn: u8,
    pub total_len: u16,
    pub identification: u16,
    pub flags_fragment: u16,
    pub ttl: u8,
    pub protocol: u8,
    pub checksum: u16,
    pub src_ip: Ipv4Addr,
    pub dst_ip: Ipv4Addr,
}

impl Ipv4Header {
    pub const MIN_LEN: usize = 20;

    pub fn parse(bytes: &[u8]) -> Option<(Self, &[u8])> {
        if bytes.len() < Self::MIN_LEN {
            return None;
        }
        let version_ihl = bytes[0];
        let total_len = u16::from_be_bytes([bytes[2], bytes[3]]);
        let protocol = bytes[9];
        let checksum = u16::from_be_bytes([bytes[10], bytes[11]]);

        let src_ip = Ipv4Addr::new(bytes[12], bytes[13], bytes[14], bytes[15]);
        let dst_ip = Ipv4Addr::new(bytes[16], bytes[17], bytes[18], bytes[19]);

        let header_len = ((version_ihl & 0x0F) * 4) as usize;
        if bytes.len() < header_len || (total_len as usize) < header_len {
            return None;
        }

        let payload_len = total_len as usize - header_len;
        if bytes.len() < header_len + payload_len {
            return None;
        }

        let payload = &bytes[header_len..header_len + payload_len];

        Some((
            Self {
                version_ihl,
                dscp_ecn: bytes[1],
                total_len,
                identification: u16::from_be_bytes([bytes[4], bytes[5]]),
                flags_fragment: u16::from_be_bytes([bytes[6], bytes[7]]),
                ttl: bytes[8],
                protocol,
                checksum,
                src_ip,
                dst_ip,
            },
            payload,
        ))
    }
}

/// ICMP Packet Header (8 bytes).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct IcmpHeader {
    pub icmp_type: u8,
    pub code: u8,
    pub checksum: u16,
    pub rest_of_header: [u8; 4],
}

impl IcmpHeader {
    pub const LEN: usize = 8;
    pub const TYPE_ECHO_REPLY: u8 = 0;
    pub const TYPE_ECHO_REQUEST: u8 = 8;

    pub fn parse(bytes: &[u8]) -> Option<(Self, &[u8])> {
        if bytes.len() < Self::LEN {
            return None;
        }
        let checksum = u16::from_be_bytes([bytes[2], bytes[3]]);
        let mut rest = [0u8; 4];
        rest.copy_from_slice(&bytes[4..8]);

        Some((
            Self {
                icmp_type: bytes[0],
                code: bytes[1],
                checksum,
                rest_of_header: rest,
            },
            &bytes[Self::LEN..],
        ))
    }
}

/// UDP Header (8 bytes).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct UdpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub length: u16,
    pub checksum: u16,
}

impl UdpHeader {
    pub const LEN: usize = 8;

    pub fn parse(bytes: &[u8]) -> Option<(Self, &[u8])> {
        if bytes.len() < Self::LEN {
            return None;
        }
        let src_port = u16::from_be_bytes([bytes[0], bytes[1]]);
        let dst_port = u16::from_be_bytes([bytes[2], bytes[3]]);
        let length = u16::from_be_bytes([bytes[4], bytes[5]]);
        let checksum = u16::from_be_bytes([bytes[6], bytes[7]]);

        if (length as usize) < Self::LEN || bytes.len() < length as usize {
            return None;
        }

        let payload = &bytes[Self::LEN..length as usize];
        Some((
            Self {
                src_port,
                dst_port,
                length,
                checksum,
            },
            payload,
        ))
    }
}

/// TCP Header (20 bytes min).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TcpHeader {
    pub src_port: u16,
    pub dst_port: u16,
    pub seq_num: u32,
    pub ack_num: u32,
    pub data_offset_reserved: u8,
    pub flags: u8,
    pub window_size: u16,
    pub checksum: u16,
    pub urgent_pointer: u16,
}

pub mod tcp_flags {
    pub const FIN: u8 = 0x01;
    pub const SYN: u8 = 0x02;
    pub const RST: u8 = 0x04;
    pub const PSH: u8 = 0x08;
    pub const ACK: u8 = 0x10;
    pub const URG: u8 = 0x20;
}

impl TcpHeader {
    pub const MIN_LEN: usize = 20;

    pub fn parse(bytes: &[u8]) -> Option<(Self, &[u8])> {
        if bytes.len() < Self::MIN_LEN {
            return None;
        }
        let src_port = u16::from_be_bytes([bytes[0], bytes[1]]);
        let dst_port = u16::from_be_bytes([bytes[2], bytes[3]]);
        let seq_num = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let ack_num = u32::from_be_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
        let data_offset_reserved = bytes[12];
        let flags = bytes[13];
        let window_size = u16::from_be_bytes([bytes[14], bytes[15]]);
        let checksum = u16::from_be_bytes([bytes[16], bytes[17]]);
        let urgent_pointer = u16::from_be_bytes([bytes[18], bytes[19]]);

        let header_len = ((data_offset_reserved >> 4) * 4) as usize;
        if bytes.len() < header_len {
            return None;
        }

        let payload = &bytes[header_len..];
        Some((
            Self {
                src_port,
                dst_port,
                seq_num,
                ack_num,
                data_offset_reserved,
                flags,
                window_size,
                checksum,
                urgent_pointer,
            },
            payload,
        ))
    }
}
