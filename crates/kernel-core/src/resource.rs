use crate::object::ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ResourceDomainId(ObjectId);

impl ResourceDomainId {
    #[cfg_attr(not(test), allow(dead_code))]
    pub const fn new(id: ObjectId) -> Self {
        Self(id)
    }

    pub const fn new_for_test(id: u32) -> Self {
        Self(ObjectId::new_for_test(id, 1))
    }

    pub const fn raw(self) -> u64 {
        self.0.raw()
    }

    pub const fn object_id(self) -> ObjectId {
        self.0
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
    StillReferenced,
}

/// Bounded accounting authority for object, capability, and physical memory creation.
#[derive(Debug, Eq, PartialEq)]
pub struct ResourceDomain {
    id: ResourceDomainId,
    parent: Option<ResourceDomainId>,
    limits: ResourceLimits,
    usage: ResourceUsage,
    process_refs: u32,
    child_domain_refs: u32,
    reservation_active: bool,
}

impl ResourceDomain {
    #[cfg_attr(not(test), allow(dead_code))]
    pub const fn new(id: ResourceDomainId, limits: ResourceLimits) -> Self {
        Self {
            id,
            parent: None,
            limits,
            usage: ResourceUsage {
                objects: 0,
                capabilities: 0,
                memory_bytes: 0,
            },
            process_refs: 0,
            child_domain_refs: 0,
            reservation_active: false,
        }
    }

    pub const fn new_for_test(id: ResourceDomainId, limits: ResourceLimits) -> Self {
        Self::new(id, limits)
    }

    pub fn new_child(
        id: ResourceDomainId,
        parent: &mut ResourceDomain,
        child_limits: ResourceLimits,
    ) -> Result<Self, ResourceError> {
        // Reserve the complete child budget atomically.  Charging one unit at
        // a time is both needlessly expensive and, more importantly, makes a
        // mid-loop failure easy to roll back incorrectly.
        let next_objects = parent
            .usage
            .objects
            .checked_add(child_limits.objects)
            .ok_or(ResourceError::ObjectLimit)?;
        let next_capabilities = parent
            .usage
            .capabilities
            .checked_add(child_limits.capabilities)
            .ok_or(ResourceError::CapabilityLimit)?;
        let next_memory = parent
            .usage
            .memory_bytes
            .checked_add(child_limits.memory_bytes)
            .ok_or(ResourceError::MemoryLimit)?;
        if next_objects > parent.limits.objects {
            return Err(ResourceError::ObjectLimit);
        }
        if next_capabilities > parent.limits.capabilities {
            return Err(ResourceError::CapabilityLimit);
        }
        if next_memory > parent.limits.memory_bytes {
            return Err(ResourceError::MemoryLimit);
        }
        parent.usage.objects = next_objects;
        parent.usage.capabilities = next_capabilities;
        parent.usage.memory_bytes = next_memory;
        parent.child_domain_refs = parent
            .child_domain_refs
            .checked_add(1)
            .ok_or(ResourceError::ObjectLimit)?;
        Ok(Self {
            id,
            parent: Some(parent.id()),
            limits: child_limits,
            usage: ResourceUsage {
                objects: 0,
                capabilities: 0,
                memory_bytes: 0,
            },
            process_refs: 0,
            child_domain_refs: 0,
            reservation_active: true,
        })
    }

    pub fn refund_to_parent(&mut self, parent: &mut ResourceDomain) -> Result<(), ResourceError> {
        if self.parent != Some(parent.id()) {
            return Err(ResourceError::StillReferenced);
        }
        if !self.reservation_active || !self.is_eligible_for_destruction() {
            return Err(ResourceError::StillReferenced);
        }
        parent.usage.objects = parent
            .usage
            .objects
            .checked_sub(self.limits.objects)
            .ok_or(ResourceError::AccountingUnderflow)?;
        parent.usage.capabilities = parent
            .usage
            .capabilities
            .checked_sub(self.limits.capabilities)
            .ok_or(ResourceError::AccountingUnderflow)?;
        parent.usage.memory_bytes = parent
            .usage
            .memory_bytes
            .checked_sub(self.limits.memory_bytes)
            .ok_or(ResourceError::AccountingUnderflow)?;
        parent.child_domain_refs = parent
            .child_domain_refs
            .checked_sub(1)
            .ok_or(ResourceError::AccountingUnderflow)?;
        self.reservation_active = false;
        Ok(())
    }

    pub fn is_eligible_for_destruction(&self) -> bool {
        self.process_refs == 0
            && self.usage.objects == 0
            && self.usage.capabilities == 0
            && self.usage.memory_bytes == 0
            && self.child_domain_refs == 0
    }

    pub const fn id(&self) -> ResourceDomainId {
        self.id
    }

    pub const fn parent(&self) -> Option<ResourceDomainId> {
        self.parent
    }

    pub const fn limits(&self) -> ResourceLimits {
        self.limits
    }

    pub const fn usage(&self) -> ResourceUsage {
        self.usage
    }

    pub fn add_process_ref(&mut self) {
        self.process_refs += 1;
    }

    pub fn release_process_ref(&mut self) -> Result<(), ResourceError> {
        if self.process_refs == 0 {
            return Err(ResourceError::AccountingUnderflow);
        }
        self.process_refs -= 1;
        Ok(())
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

    fn test_id(i: u32) -> ResourceDomainId {
        ResourceDomainId::new_for_test(i)
    }

    #[test]
    fn resource_domain_memory_quota_charge_and_rollback() {
        let mut domain = ResourceDomain::new(
            test_id(1),
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

    #[test]
    fn child_domain_reservation_and_rollback() {
        let mut parent = ResourceDomain::new(
            test_id(1),
            ResourceLimits {
                objects: 10,
                capabilities: 10,
                memory_bytes: 65536,
            },
        );

        let child_limits = ResourceLimits {
            objects: 2,
            capabilities: 5,
            memory_bytes: 4096,
        };

        let child = ResourceDomain::new_child(test_id(2), &mut parent, child_limits).unwrap();

        assert_eq!(parent.usage().objects, 2);
        assert_eq!(parent.usage().capabilities, 5);
        assert_eq!(parent.usage().memory_bytes, 4096);
        assert_eq!(parent.child_domain_refs, 1);
        assert_eq!(child.parent(), Some(parent.id()));

        // Refund
        let mut child = child;
        child.refund_to_parent(&mut parent).unwrap();
        assert_eq!(parent.usage().objects, 0);
        assert_eq!(parent.usage().capabilities, 0);
        assert_eq!(parent.usage().memory_bytes, 0);
        assert_eq!(parent.child_domain_refs, 0);
    }
}
