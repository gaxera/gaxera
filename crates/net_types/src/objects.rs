//! First-Class Native Network Objects & Identifiers.

use crate::address::{IpAddr, MacAddress};
use gaxera_abi::GaxObjectId;

/// Protocol Family for Endpoints.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum ProtocolFamily {
    Unspecified = 0,
    Ipv4 = 4,
    Ipv6 = 6,
}

/// Network Endpoint Identifier Object (`NetEndpoint`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct NetEndpoint {
    pub ip: IpAddr,
    pub port: u16,
    pub family: ProtocolFamily,
    pub interface_id: Option<GaxObjectId>,
}

impl NetEndpoint {
    pub fn new(ip: IpAddr, port: u16) -> Self {
        let family = match ip {
            IpAddr::V4(_) => ProtocolFamily::Ipv4,
            IpAddr::V6(_) => ProtocolFamily::Ipv6,
        };
        Self {
            ip,
            port,
            family,
            interface_id: None,
        }
    }
}

/// Link Status for Interfaces.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum LinkStatus {
    Down = 0,
    Up = 1,
    Testing = 2,
    Unknown = 3,
}

/// Network Interface Metadata Object (`NetInterface`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct NetInterface {
    pub id: GaxObjectId,
    pub name: [u8; 16],
    pub mac: MacAddress,
    pub ip: Option<IpAddr>,
    pub mtu: u32,
    pub link_status: LinkStatus,
}

/// Network Route Entry Object (`NetRoute`).
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct NetRoute {
    pub id: GaxObjectId,
    pub destination_cidr: crate::address::IpCidr,
    pub gateway: Option<IpAddr>,
    pub interface_id: GaxObjectId,
    pub metric: u32,
}

/// First-Class Network Object Enum (`GaxNetObject`).
#[derive(Clone, Debug)]
pub enum GaxNetObject {
    Namespace(GaxObjectId),
    Interface(NetInterface),
    Listener(GaxObjectId), // NetListener ID
    Endpoint(NetEndpoint),
    Session(GaxObjectId), // NetSession ID
    Route(NetRoute),
}
