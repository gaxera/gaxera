//! Subsystem-Isolated Network Rights and Policy Constraints.

use crate::address::IpCidr;
use alloc::string::String;
use alloc::vec::Vec;
use core::ops::RangeInclusive;

/// Subsystem-isolated Network Rights Bitfield (`NetRights`).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(transparent)]
pub struct NetRights(pub u64);

impl NetRights {
    pub const NONE: Self = Self(0);
    pub const READ: Self = Self(1 << 0); // Read incoming payload packets
    pub const WRITE: Self = Self(1 << 1); // Write outgoing payload packets
    pub const CONTROL: Self = Self(1 << 2); // Modify session options / close
    pub const CONNECT: Self = Self(1 << 16); // Outbound session creation
    pub const LISTEN: Self = Self(1 << 17); // Inbound server port binding
    pub const BIND_RAW: Self = Self(1 << 18); // Low-port (<1024) binding
    pub const RESOLVE: Self = Self(1 << 19); // Domain name lookup via DNS
    pub const MULTICAST: Self = Self(1 << 20); // IGMP / Multicast group join
    pub const LAN_ONLY: Self = Self(1 << 21); // Restrict to RFC 1918 private subnets
    pub const PROMISCUOUS: Self = Self(1 << 22); // Raw frame capture (Admin only)

    /// Check if this `NetRights` set contains all required rights.
    pub fn contains(&self, required: Self) -> bool {
        (self.0 & required.0) == required.0
    }

    /// Monotonic attenuation: derive child rights containing at most `derived` rights.
    pub fn derive_narrowed(&self, desired: Self) -> Self {
        Self(self.0 & desired.0)
    }
}

/// FQDN Domain Pattern matching rule (e.g. `*.api.gaxera.org`).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct DomainPattern(pub String);

impl DomainPattern {
    /// Check if a given FQDN domain string matches this pattern.
    pub fn matches(&self, domain: &str) -> bool {
        if self.0 == "*" {
            return true;
        }
        if let Some(suffix) = self.0.strip_prefix("*.") {
            return domain.ends_with(suffix) || domain == suffix;
        }
        self.0.eq_ignore_ascii_case(domain)
    }
}

/// Capability Scoping Constraints (`NetCapabilityPolicy`).
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub struct NetCapabilityPolicy {
    /// Permitted destination IP CIDRs (empty = unscoped / all allowed).
    pub allowed_cidrs: Vec<IpCidr>,
    /// Permitted destination FQDN domain patterns.
    pub allowed_domains: Vec<DomainPattern>,
    /// Permitted port ranges (empty = unscoped).
    pub port_ranges: Vec<RangeInclusive<u16>>,
    /// Bandwidth quota in bytes per second (0 = unlimited).
    pub max_bandwidth_bps: u64,
    /// Maximum allowed concurrent active sessions (0 = unlimited).
    pub max_concurrent_sessions: u32,
    /// Lease expiration epoch timestamp (None = non-expiring).
    pub lease_expires_at: Option<u64>,
}

impl NetCapabilityPolicy {
    /// Check if a target port is permitted by this policy.
    pub fn allows_port(&self, port: u16) -> bool {
        if self.port_ranges.is_empty() {
            return true;
        }
        self.port_ranges.iter().any(|range| range.contains(&port))
    }

    /// Check if a target FQDN domain is permitted by this policy.
    pub fn allows_domain(&self, domain: &str) -> bool {
        if self.allowed_domains.is_empty() {
            return true;
        }
        self.allowed_domains
            .iter()
            .any(|pattern| pattern.matches(domain))
    }
}
