#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ResourceDomainId(u32);

impl ResourceDomainId {
    // Domain creation enters the kernel with the later bootstrap-object path.
    // M1 exercises the constructor only in this crate's host model.
    // M1 exercises the constructor only in this crate's host model.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    pub const fn new_for_test(raw: u32) -> Self {
        Self(raw)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    pub objects: u32,
    pub capabilities: u32,
    pub memory_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceUsage {
    pub objects: u32,
    pub capabilities: u32,
    pub memory_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceError {
    ObjectLimit,
    CapabilityLimit,
    MemoryLimit,
    AccountingUnderflow,
}

/// Bounded accounting authority for object, capability, and physical memory creation.
///
/// The domain deliberately has no physical-address or page-table authority.
#[derive(Debug, Eq, PartialEq)]
pub struct ResourceDomain {
    id: ResourceDomainId,
    limits: ResourceLimits,
    usage: ResourceUsage,
}

impl ResourceDomain {
    // M1 deliberately has no public domain-construction path.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn new(id: ResourceDomainId, limits: ResourceLimits) -> Self {
        Self {
            id,
            limits,
            usage: ResourceUsage {
                objects: 0,
                capabilities: 0,
                memory_bytes: 0,
            },
        }
    }

    pub const fn new_for_test(id: ResourceDomainId, limits: ResourceLimits) -> Self {
        Self::new(id, limits)
    }

    pub const fn id(&self) -> ResourceDomainId {
        self.id
    }

    pub const fn limits(&self) -> ResourceLimits {
        self.limits
    }

    pub const fn usage(&self) -> ResourceUsage {
        self.usage
    }

    pub fn charge_object(&mut self) -> Result<(), ResourceError> {
        if self.usage.objects >= self.limits.objects {
            return Err(ResourceError::ObjectLimit);
        }
        self.usage.objects += 1;
        Ok(())
    }

    pub fn release_object(&mut self) -> Result<(), ResourceError> {
        if self.usage.objects == 0 {
            return Err(ResourceError::AccountingUnderflow);
        }
        self.usage.objects -= 1;
        Ok(())
    }

    pub fn charge_capability(&mut self) -> Result<(), ResourceError> {
        if self.usage.capabilities >= self.limits.capabilities {
            return Err(ResourceError::CapabilityLimit);
        }
        self.usage.capabilities += 1;
        Ok(())
    }

    pub fn release_capability(&mut self) -> Result<(), ResourceError> {
        if self.usage.capabilities == 0 {
            return Err(ResourceError::AccountingUnderflow);
        }
        self.usage.capabilities -= 1;
        Ok(())
    }

    pub fn charge_memory(&mut self, bytes: u64) -> Result<(), ResourceError> {
        let new_usage = self
            .usage
            .memory_bytes
            .checked_add(bytes)
            .ok_or(ResourceError::MemoryLimit)?;
        if new_usage > self.limits.memory_bytes {
            return Err(ResourceError::MemoryLimit);
        }
        self.usage.memory_bytes = new_usage;
        Ok(())
    }

    pub fn rollback_memory(&mut self, bytes: u64) -> Result<(), ResourceError> {
        let new_usage = self
            .usage
            .memory_bytes
            .checked_sub(bytes)
            .ok_or(ResourceError::AccountingUnderflow)?;
        self.usage.memory_bytes = new_usage;
        Ok(())
    }

    pub fn release_memory(&mut self, bytes: u64) -> Result<(), ResourceError> {
        self.rollback_memory(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_domain_memory_quota_charge_and_rollback() {
        let mut domain = ResourceDomain::new(
            ResourceDomainId::new(1),
            ResourceLimits {
                objects: 10,
                capabilities: 10,
                memory_bytes: 65536,
            },
        );

        assert_eq!(domain.usage().memory_bytes, 0);

        // Charge valid 32 KiB
        assert!(domain.charge_memory(32768).is_ok());
        assert_eq!(domain.usage().memory_bytes, 32768);

        // Charge another 32 KiB -> hits exact limit
        assert!(domain.charge_memory(32768).is_ok());
        assert_eq!(domain.usage().memory_bytes, 65536);

        // Charge 1 more byte -> MemoryLimit error, state unchanged
        assert_eq!(domain.charge_memory(1), Err(ResourceError::MemoryLimit));
        assert_eq!(domain.usage().memory_bytes, 65536);

        // Rollback 32 KiB
        assert!(domain.rollback_memory(32768).is_ok());
        assert_eq!(domain.usage().memory_bytes, 32768);

        // Release remaining 32 KiB
        assert!(domain.release_memory(32768).is_ok());
        assert_eq!(domain.usage().memory_bytes, 0);

        // Underflow error on release below zero
        assert_eq!(
            domain.release_memory(1),
            Err(ResourceError::AccountingUnderflow)
        );
        assert_eq!(domain.usage().memory_bytes, 0);
    }
}
