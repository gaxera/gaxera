#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ResourceDomainId(u32);

impl ResourceDomainId {
    // Domain creation enters the kernel with the later bootstrap-object path.
    // M1 exercises the constructor only in this crate's host model.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    pub objects: u32,
    pub capabilities: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceUsage {
    pub objects: u32,
    pub capabilities: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceError {
    ObjectLimit,
    CapabilityLimit,
    AccountingUnderflow,
}

/// Bounded accounting authority for object and capability creation.
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
            },
        }
    }

    pub(crate) const fn id(&self) -> ResourceDomainId {
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
}
