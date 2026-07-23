use core::fmt;

pub const MAX_SERVICE_NAME_LEN: usize = 32;

/// A transparent fixed-size 32-byte wire representation of a service name.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct ServiceName([u8; MAX_SERVICE_NAME_LEN]);

impl ServiceName {
    pub const EMPTY: Self = Self([0u8; MAX_SERVICE_NAME_LEN]);

    /// Attempt to construct a `ServiceName` from a string slice.
    /// Returns `Err(ServiceStatus::InvalidName)` if the string is empty or exceeds 32 bytes.
    pub fn try_from_str(s: &str) -> Result<Self, ServiceStatus> {
        let bytes = s.as_bytes();
        if bytes.is_empty() || bytes.len() > MAX_SERVICE_NAME_LEN {
            return Err(ServiceStatus::InvalidName);
        }
        let mut buf = [0u8; MAX_SERVICE_NAME_LEN];
        buf[..bytes.len()].copy_from_slice(bytes);
        Ok(Self(buf))
    }

    /// Access the underlying raw 32-byte array.
    pub const fn as_bytes(&self) -> &[u8; MAX_SERVICE_NAME_LEN] {
        &self.0
    }

    /// Convert to string slice up to the first null byte or length 32.
    pub fn as_str(&self) -> &str {
        let len = self
            .0
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(MAX_SERVICE_NAME_LEN);
        core::str::from_utf8(&self.0[..len]).unwrap_or("")
    }
}

impl fmt::Display for ServiceName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum ServiceOp {
    Register = 1,
    Lookup = 2,
    Response = 3,
    Unregister = 4, // Reserved for future service supervision
}

impl ServiceOp {
    pub fn from_u16(val: u16) -> Option<Self> {
        match val {
            1 => Some(Self::Register),
            2 => Some(Self::Lookup),
            3 => Some(Self::Response),
            4 => Some(Self::Unregister),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ServiceStatus {
    Success = 0,
    NotFound = 1,
    AlreadyExists = 2,
    AccessDenied = 3,
    RegistryFull = 4,
    InvalidName = 5,
    UnknownOp = 6,
}

impl ServiceStatus {
    pub fn from_u32(val: u32) -> Self {
        match val {
            0 => Self::Success,
            1 => Self::NotFound,
            2 => Self::AlreadyExists,
            3 => Self::AccessDenied,
            4 => Self::RegistryFull,
            5 => Self::InvalidName,
            _other => Self::UnknownOp,
        }
    }
}

/// Extensible 64-bit service discovery protocol header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct ServiceHeader {
    pub version: u16,
    pub op: u16,
    pub status: u32,
}

impl ServiceHeader {
    pub const VERSION_1: u16 = 1;

    pub const fn new(op: ServiceOp, status: ServiceStatus) -> Self {
        Self {
            version: Self::VERSION_1,
            op: op as u16,
            status: status as u32,
        }
    }
}

/// Service registration IPC request message format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct ServiceRegisterReq {
    pub header: ServiceHeader,
    pub name: ServiceName,
}

impl ServiceRegisterReq {
    pub fn try_decode(payload: &[u8]) -> Result<Self, ServiceStatus> {
        if payload.len() < core::mem::size_of::<Self>() {
            return Err(ServiceStatus::UnknownOp);
        }
        let mut buf = [0u8; core::mem::size_of::<Self>()];
        buf.copy_from_slice(&payload[..core::mem::size_of::<Self>()]);
        // SAFETY: Self is repr(C) POD with valid byte layout.
        let req = unsafe { core::mem::transmute::<[u8; core::mem::size_of::<Self>()], Self>(buf) };
        if req.header.version != ServiceHeader::VERSION_1 {
            return Err(ServiceStatus::UnknownOp);
        }
        Ok(req)
    }
}

/// Service lookup IPC request message format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct ServiceLookupReq {
    pub header: ServiceHeader,
    pub name: ServiceName,
}

impl ServiceLookupReq {
    pub fn try_decode(payload: &[u8]) -> Result<Self, ServiceStatus> {
        if payload.len() < core::mem::size_of::<Self>() {
            return Err(ServiceStatus::UnknownOp);
        }
        let mut buf = [0u8; core::mem::size_of::<Self>()];
        buf.copy_from_slice(&payload[..core::mem::size_of::<Self>()]);
        // SAFETY: Self is repr(C) POD with valid byte layout.
        let req = unsafe { core::mem::transmute::<[u8; core::mem::size_of::<Self>()], Self>(buf) };
        if req.header.version != ServiceHeader::VERSION_1 {
            return Err(ServiceStatus::UnknownOp);
        }
        Ok(req)
    }
}

/// Service lookup/registration IPC response message format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct ServiceResponse {
    pub header: ServiceHeader,
    pub name: ServiceName,
}

impl ServiceResponse {
    pub fn try_decode(payload: &[u8]) -> Result<Self, ServiceStatus> {
        if payload.len() < core::mem::size_of::<Self>() {
            return Err(ServiceStatus::UnknownOp);
        }
        let mut buf = [0u8; core::mem::size_of::<Self>()];
        buf.copy_from_slice(&payload[..core::mem::size_of::<Self>()]);
        // SAFETY: Self is repr(C) POD with valid byte layout.
        let resp = unsafe { core::mem::transmute::<[u8; core::mem::size_of::<Self>()], Self>(buf) };
        if resp.header.version != ServiceHeader::VERSION_1 {
            return Err(ServiceStatus::UnknownOp);
        }
        Ok(resp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_service_wire_roundtrip_and_malformed_rejection() {
        let name = ServiceName::try_from_str("test.service").unwrap();
        let resp = ServiceResponse {
            header: ServiceHeader::new(ServiceOp::Response, ServiceStatus::Success),
            name,
        };
        // SAFETY: POD struct layout
        let wire = unsafe {
            core::slice::from_raw_parts(
                &resp as *const _ as *const u8,
                core::mem::size_of::<ServiceResponse>(),
            )
        };
        let decoded = ServiceResponse::try_decode(wire).unwrap();
        assert_eq!(decoded.name.as_str(), "test.service");
        assert_eq!(decoded.header.version, ServiceHeader::VERSION_1);

        // Truncated payload rejection
        assert_eq!(
            ServiceResponse::try_decode(&wire[..10]),
            Err(ServiceStatus::UnknownOp)
        );

        // Invalid version rejection
        let mut invalid_wire = wire.to_vec();
        invalid_wire[0] = 0x99; // invalid version
        assert_eq!(
            ServiceResponse::try_decode(&invalid_wire),
            Err(ServiceStatus::UnknownOp)
        );
    }
}
