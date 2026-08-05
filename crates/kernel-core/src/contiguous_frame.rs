use crate::object::ObjectId;
use crate::resource::ResourceDomainId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContiguousFrameError {
    CapabilityOverflow,
    CapabilityUnderflow,
    MappingOverflow,
    MappingUnderflow,
}

/// A kernel-owned, capability-authorized contiguous DMA frame object.
#[derive(Debug, Eq, PartialEq)]
pub struct ContiguousFrameObject {
    id: ObjectId,
    base_frame: u64,
    page_count: usize,
    order: u8,
    owner: ResourceDomainId,
    capability_count: u32,
    mapping_count: usize,
}

impl ContiguousFrameObject {
    pub fn new(
        id: ObjectId,
        base_frame: u64,
        page_count: usize,
        order: u8,
        owner: ResourceDomainId,
    ) -> Self {
        Self {
            id,
            base_frame,
            page_count,
            order,
            owner,
            capability_count: 1,
            mapping_count: 0,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn base_frame(&self) -> u64 {
        self.base_frame
    }

    pub fn page_count(&self) -> usize {
        self.page_count
    }

    pub fn order(&self) -> u8 {
        self.order
    }

    pub fn owner(&self) -> ResourceDomainId {
        self.owner
    }

    pub fn mapping_count(&self) -> usize {
        self.mapping_count
    }

    pub fn capability_count(&self) -> u32 {
        self.capability_count
    }

    pub fn inc_capability(&mut self) -> Result<(), ContiguousFrameError> {
        self.capability_count = self
            .capability_count
            .checked_add(1)
            .ok_or(ContiguousFrameError::CapabilityOverflow)?;
        Ok(())
    }

    pub fn dec_capability(&mut self) -> Result<bool, ContiguousFrameError> {
        self.capability_count = self
            .capability_count
            .checked_sub(1)
            .ok_or(ContiguousFrameError::CapabilityUnderflow)?;
        Ok(self.can_destroy())
    }

    pub fn can_destroy(&self) -> bool {
        self.capability_count == 0 && self.mapping_count == 0
    }

    pub fn is_order_aligned(&self) -> bool {
        let expected_count = 1usize << self.order;
        let alignment = (expected_count as u64) * 4096;
        self.page_count == expected_count && self.base_frame.is_multiple_of(alignment)
    }

    pub fn add_mapping(&mut self) -> Result<(), ContiguousFrameError> {
        self.mapping_count = self
            .mapping_count
            .checked_add(1)
            .ok_or(ContiguousFrameError::MappingOverflow)?;
        Ok(())
    }

    pub fn remove_mapping(&mut self) -> Result<(), ContiguousFrameError> {
        self.mapping_count = self
            .mapping_count
            .checked_sub(1)
            .ok_or(ContiguousFrameError::MappingUnderflow)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_frame_properties_and_mapping_counts() {
        let mut frame = ContiguousFrameObject::new(
            ObjectId::from_raw(1),
            0x10000,
            4,
            2,
            ResourceDomainId::new_for_test(1),
        );
        assert_eq!(frame.base_frame(), 0x10000);
        assert_eq!(frame.page_count(), 4);
        assert_eq!(frame.order(), 2);
        assert_eq!(frame.mapping_count(), 0);
        assert_eq!(frame.capability_count(), 1);

        frame.add_mapping().unwrap();
        assert_eq!(frame.mapping_count(), 1);
        frame.remove_mapping().unwrap();
        assert_eq!(frame.mapping_count(), 0);

        assert!(frame.is_order_aligned());

        let misaligned_frame = ContiguousFrameObject::new(
            ObjectId::from_raw(2),
            0x11000, // Not 16 KiB aligned
            4,
            2,
            ResourceDomainId::new_for_test(1),
        );
        assert!(!misaligned_frame.is_order_aligned());
    }
}
