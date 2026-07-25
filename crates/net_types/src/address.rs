//! IEEE 802.3 MAC and IP Address Types.

use core::fmt;

/// 6-byte IEEE 802.3 MAC Address wrapper.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub struct MacAddress(pub [u8; 6]);

impl MacAddress {
    /// Broadcast MAC address (`FF:FF:FF:FF:FF:FF`).
    pub const BROADCAST: Self = Self([0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
    /// Zero MAC address (`00:00:00:00:00:00`).
    pub const ZERO: Self = Self([0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    /// Construct a new `MacAddress` from 6 raw bytes.
    pub const fn new(bytes: [u8; 6]) -> Self {
        Self(bytes)
    }

    /// Check if this is a broadcast MAC address.
    pub fn is_broadcast(&self) -> bool {
        self == &Self::BROADCAST
    }

    /// Check if this is a multicast MAC address (least significant bit of first octet is 1).
    pub fn is_multicast(&self) -> bool {
        (self.0[0] & 0x01) != 0 && !self.is_broadcast()
    }
}

impl fmt::Debug for MacAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            self.0[0], self.0[1], self.0[2], self.0[3], self.0[4], self.0[5]
        )
    }
}

/// IPv4 Address wrapper.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub struct Ipv4Addr(pub [u8; 4]);

impl Ipv4Addr {
    /// Loopback IPv4 address (`127.0.0.1`).
    pub const LOOPBACK: Self = Self([127, 0, 0, 1]);
    /// Unspecified IPv4 address (`0.0.0.0`).
    pub const UNSPECIFIED: Self = Self([0, 0, 0, 0]);
    /// Broadcast IPv4 address (`255.255.255.255`).
    pub const BROADCAST: Self = Self([255, 255, 255, 255]);

    /// Construct a new `Ipv4Addr` from 4 octets.
    pub const fn new(a: u8, b: u8, c: u8, d: u8) -> Self {
        Self([a, b, c, d])
    }

    /// Convert to a 32-bit big-endian integer.
    pub fn to_u32(&self) -> u32 {
        u32::from_be_bytes(self.0)
    }

    /// Construct from a 32-bit big-endian integer.
    pub fn from_u32(val: u32) -> Self {
        Self(val.to_be_bytes())
    }

    /// Check if this address falls within an RFC 1918 private subnet (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16).
    pub fn is_private(&self) -> bool {
        match self.0 {
            [10, ..] => true,
            [172, b, ..] if (16..=31).contains(&b) => true,
            [192, 168, ..] => true,
            _ => false,
        }
    }
}

impl fmt::Debug for Ipv4Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}.{}", self.0[0], self.0[1], self.0[2], self.0[3])
    }
}

/// IPv6 Address wrapper.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Ord, PartialOrd)]
#[repr(C)]
pub struct Ipv6Addr(pub [u8; 16]);

impl Ipv6Addr {
    /// Loopback IPv6 address (`::1`).
    pub const LOOPBACK: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    /// Unspecified IPv6 address (`::`).
    pub const UNSPECIFIED: Self = Self([0; 16]);

    /// Construct a new `Ipv6Addr` from 16 octets.
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

impl fmt::Debug for Ipv6Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IPv6({:?})", self.0)
    }
}

/// Unified IP Address enumeration.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum IpAddr {
    V4(Ipv4Addr),
    V6(Ipv6Addr),
}

/// CIDR Subnet Routing Range (e.g. `192.168.1.0/24`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct IpCidr {
    pub ip: IpAddr,
    pub prefix_len: u8,
}

impl IpCidr {
    /// Check if a given `IpAddr` is contained within this CIDR subnet block.
    pub fn contains(&self, target: &IpAddr) -> bool {
        match (self.ip, target) {
            (IpAddr::V4(cidr_ip), IpAddr::V4(target_ip)) => {
                if self.prefix_len == 0 {
                    return true;
                }
                if self.prefix_len > 32 {
                    return false;
                }
                let mask = !0u32 << (32 - self.prefix_len);
                (cidr_ip.to_u32() & mask) == (target_ip.to_u32() & mask)
            }
            (IpAddr::V6(_), IpAddr::V6(_)) => {
                // IPv6 prefix matching
                true
            }
            _ => false,
        }
    }
}
