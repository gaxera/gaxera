use crate::object::ObjectId;
use crate::resource::ResourceDomainId;

/// A kernel-owned, capability-authorized contiguous DMA frame object.
#[derive(Debug, Eq, PartialEq)]
pub struct ContiguousFrameObject {
    id: ObjectId,
    base_frame: u64,
    page_count: usize,
    order: u8,
    owner: ResourceDomainId,
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

    pub fn add_mapping(&mut self) {
        self.mapping_count += 1;
    }

    pub fn remove_mapping(&mut self) {
        self.mapping_count = self.mapping_count.saturating_sub(1);
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
            ResourceDomainId::new(1),
        );
        assert_eq!(frame.base_frame(), 0x10000);
        assert_eq!(frame.page_count(), 4);
        assert_eq!(frame.order(), 2);
        assert_eq!(frame.mapping_count(), 0);

        frame.add_mapping();
        assert_eq!(frame.mapping_count(), 1);
        frame.remove_mapping();
        assert_eq!(frame.mapping_count(), 0);
    }
}
