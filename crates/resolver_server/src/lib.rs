//! Ring-3 Domain Resolution Service (`resolver_server`).

#![no_std]

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use net_types::{IpAddr, Ipv4Addr, ProviderError, ResolverProvider};

pub struct DnsResolverServer;

impl ResolverProvider for DnsResolverServer {
    fn resolve_domain(&self, domain: &str) -> Result<Vec<IpAddr>, ProviderError> {
        if domain == "localhost" {
            Ok(vec![IpAddr::V4(Ipv4Addr::LOOPBACK)])
        } else if domain == "gaxera.org" {
            Ok(vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))])
        } else {
            Err(ProviderError::ResolverError)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_domain_resolution() {
        let resolver = DnsResolverServer;
        let res = resolver.resolve_domain("localhost").unwrap();
        assert_eq!(res[0], IpAddr::V4(Ipv4Addr::LOOPBACK));
    }
}
