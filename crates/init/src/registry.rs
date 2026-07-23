use gaxera_abi::service::{ServiceName, ServiceStatus};
use libgaxera::object::endpoint::EndpointHandle;

pub const MAX_SERVICES: usize = 32;

#[allow(dead_code)]
pub struct RegistryEntry {
    pub name: ServiceName,
    pub endpoint: EndpointHandle,
}

pub struct ServiceRegistry {
    entries: [Option<RegistryEntry>; MAX_SERVICES],
    count: usize,
}

#[allow(dead_code)]
impl ServiceRegistry {
    pub const fn new() -> Self {
        const INIT_NONE: Option<RegistryEntry> = None;
        Self {
            entries: [INIT_NONE; MAX_SERVICES],
            count: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Register a service endpoint by name.
    pub fn register(
        &mut self,
        name: ServiceName,
        endpoint: EndpointHandle,
    ) -> Result<(), ServiceStatus> {
        // Check for duplicates
        for slot in self.entries.iter().flatten() {
            if slot.name == name {
                return Err(ServiceStatus::AlreadyExists);
            }
        }

        if self.count >= MAX_SERVICES {
            return Err(ServiceStatus::RegistryFull);
        }

        // Find empty slot and insert
        for slot in self.entries.iter_mut() {
            if slot.is_none() {
                *slot = Some(RegistryEntry { name, endpoint });
                self.count += 1;
                return Ok(());
            }
        }

        Err(ServiceStatus::RegistryFull)
    }

    /// Lookup a service endpoint by name.
    #[allow(dead_code)]
    pub fn lookup(&self, name: &ServiceName) -> Option<&EndpointHandle> {
        for slot in self.entries.iter().flatten() {
            if slot.name == *name {
                return Some(&slot.endpoint);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaxera_abi::Handle;

    #[test]
    fn test_service_registry_registration_and_duplicate_rejection() {
        let mut registry = ServiceRegistry::new();
        let name = ServiceName::try_from_str("gaxera.test.service").unwrap();
        let ep = EndpointHandle::from_raw(Handle::from_parts(10, 1));

        // 1. Successful registration
        assert!(registry.register(name, ep).is_ok());
        assert_eq!(registry.len(), 1);

        // 2. Duplicate registration rejection
        assert_eq!(
            registry.register(name, ep),
            Err(ServiceStatus::AlreadyExists)
        );

        // 3. Lookup returns registered endpoint
        let looked_up = registry.lookup(&name).unwrap();
        assert_eq!(looked_up.as_handle(), Handle::from_parts(10, 1));
    }
}
