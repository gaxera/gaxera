use gaxera_abi::CachePolicy;

use crate::capability::CapabilityNodeId;
use crate::object::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingError {
    InvalidAlignment,
    ZeroSize,
    Closed,
}

/// Backing representation for a Mapping bridge.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingBacking {
    MemoryObject {
        object_id: ObjectId,
        offset_bytes: u64,
    },
    PhysicalRange {
        physical_base: u64,
        cache_policy: CachePolicy,
    },
    ContiguousFrame {
        object_id: ObjectId,
        physical_base: u64,
        offset_bytes: u64,
        cache_policy: CachePolicy,
    },
}

/// Pure Explicit Bridge Mapping Capability (`ObjectType::Mapping = 6`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mapping {
    id: ObjectId,
    target_address_space: ObjectId,
    virtual_address: u64,
    size: usize,
    permissions: gaxera_abi::Rights,
    backing: MappingBacking,
    lineage_parent_node: Option<CapabilityNodeId>,
    capability_refs: u32,
    closed: bool,
}

impl Mapping {
    /// Create a MemoryObject bridge Mapping.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new_memory_object(
        id: ObjectId,
        target_address_space: ObjectId,
        virtual_address: u64,
        source_memory_object: ObjectId,
        offset_bytes: u64,
        size: usize,
        permissions: gaxera_abi::Rights,
        lineage_parent_node: Option<CapabilityNodeId>,
    ) -> Result<Self, MappingError> {
        if virtual_address & 0xFFF != 0 || offset_bytes & 0xFFF != 0 {
            return Err(MappingError::InvalidAlignment);
        }
        if size == 0 || (size & 0xFFF) != 0 {
            return Err(MappingError::ZeroSize);
        }
        Ok(Self {
            id,
            target_address_space,
            virtual_address,
            size,
            permissions,
            backing: MappingBacking::MemoryObject {
                object_id: source_memory_object,
                offset_bytes,
            },
            lineage_parent_node,
            capability_refs: 1,
            closed: false,
        })
    }

    /// Backwards compatible 4-argument constructor for MMIO ranges.
    pub fn try_new(
        id: ObjectId,
        phys_addr: u64,
        size: usize,
        cache_policy: CachePolicy,
    ) -> Result<Self, MappingError> {
        Self::try_new_mmio(
            id,
            ObjectId::new_for_test(0, 0),
            0,
            phys_addr,
            size,
            cache_policy,
            gaxera_abi::Rights::MAP | gaxera_abi::Rights::READ | gaxera_abi::Rights::WRITE,
        )
    }

    pub fn try_new_mmio(
        id: ObjectId,
        target_address_space: ObjectId,
        virtual_address: u64,
        physical_base: u64,
        size: usize,
        cache_policy: CachePolicy,
        permissions: gaxera_abi::Rights,
    ) -> Result<Self, MappingError> {
        if (virtual_address & 0xFFF != 0) || (physical_base & 0xFFF != 0) {
            return Err(MappingError::InvalidAlignment);
        }
        if size == 0 || (size & 0xFFF) != 0 {
            return Err(MappingError::ZeroSize);
        }
        Ok(Self {
            id,
            target_address_space,
            virtual_address,
            size,
            permissions,
            backing: MappingBacking::PhysicalRange {
                physical_base,
                cache_policy,
            },
            lineage_parent_node: None,
            capability_refs: 1,
            closed: false,
        })
    }

    // The constructor keeps the physical frame, offset, virtual range, and
    // permission contract explicit at this kernel-object boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new_contiguous_frame(
        id: ObjectId,
        target_address_space: ObjectId,
        virtual_address: u64,
        frame_object: ObjectId,
        physical_base: u64,
        offset_bytes: u64,
        size: usize,
        permissions: gaxera_abi::Rights,
    ) -> Result<Self, MappingError> {
        if (virtual_address & 0xFFF != 0)
            || (physical_base & 0xFFF != 0)
            || (offset_bytes & 0xFFF != 0)
        {
            return Err(MappingError::InvalidAlignment);
        }
        if size == 0 || (size & 0xFFF) != 0 {
            return Err(MappingError::ZeroSize);
        }
        Ok(Self {
            id,
            target_address_space,
            virtual_address,
            size,
            permissions,
            backing: MappingBacking::ContiguousFrame {
                object_id: frame_object,
                physical_base,
                offset_bytes,
                cache_policy: CachePolicy::Cached,
            },
            lineage_parent_node: None,
            capability_refs: 1,
            closed: false,
        })
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn target_address_space(&self) -> ObjectId {
        self.target_address_space
    }

    pub fn virtual_address(&self) -> u64 {
        self.virtual_address
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn permissions(&self) -> gaxera_abi::Rights {
        self.permissions
    }

    pub fn backing(&self) -> &MappingBacking {
        &self.backing
    }

    pub fn phys_addr(&self) -> Option<u64> {
        match self.backing {
            MappingBacking::PhysicalRange { physical_base, .. } => Some(physical_base),
            MappingBacking::ContiguousFrame {
                physical_base,
                offset_bytes,
                ..
            } => Some(physical_base + offset_bytes),
            MappingBacking::MemoryObject { .. } => None,
        }
    }

    pub fn cache_policy(&self) -> CachePolicy {
        match self.backing {
            MappingBacking::PhysicalRange { cache_policy, .. } => cache_policy,
            MappingBacking::ContiguousFrame { cache_policy, .. } => cache_policy,
            MappingBacking::MemoryObject { .. } => CachePolicy::Cached,
        }
    }

    pub fn lineage_parent_node(&self) -> Option<CapabilityNodeId> {
        self.lineage_parent_node
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn inc_capability_ref(&mut self) -> Result<(), MappingError> {
        self.capability_refs = self
            .capability_refs
            .checked_add(1)
            .ok_or(MappingError::Closed)?;
        Ok(())
    }

    pub fn dec_capability_ref(&mut self) -> Result<bool, MappingError> {
        self.capability_refs = self
            .capability_refs
            .checked_sub(1)
            .ok_or(MappingError::Closed)?;
        Ok(self.capability_refs == 0)
    }

    pub fn capability_refs(&self) -> u32 {
        self.capability_refs
    }

    pub fn close(&mut self) -> Result<(), MappingError> {
        if self.closed {
            return Err(MappingError::Closed);
        }
        self.closed = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaxera_abi::Rights;

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn mapping_bridge_creation_and_validation() {
        let mem_obj_id = test_id(10);
        let aspace_id = test_id(20);
        let parent_node = CapabilityNodeId {
            index: 1,
            generation: 1,
        };

        // Valid page-aligned 64 KiB mapping bridge
        let mapping = Mapping::try_new_memory_object(
            test_id(1),
            aspace_id,
            0x0000_6000_0000_0000,
            mem_obj_id,
            0,
            65536,
            Rights::MAP | Rights::READ | Rights::WRITE,
            Some(parent_node),
        )
        .unwrap();

        assert_eq!(
            mapping.backing(),
            &MappingBacking::MemoryObject {
                object_id: mem_obj_id,
                offset_bytes: 0
            }
        );
        assert_eq!(mapping.target_address_space(), aspace_id);
        assert_eq!(mapping.virtual_address(), 0x0000_6000_0000_0000);
        assert_eq!(mapping.size(), 65536);
        assert_eq!(mapping.lineage_parent_node(), Some(parent_node));
        assert!(!mapping.is_closed());

        // Misaligned virtual address rejected
        assert_eq!(
            Mapping::try_new_memory_object(
                test_id(2),
                aspace_id,
                0x0000_6000_0000_0100,
                mem_obj_id,
                0,
                4096,
                Rights::READ,
                None,
            ),
            Err(MappingError::InvalidAlignment)
        );

        // Misaligned offset rejected
        assert_eq!(
            Mapping::try_new_memory_object(
                test_id(3),
                aspace_id,
                0x0000_6000_0000_0000,
                mem_obj_id,
                100,
                4096,
                Rights::READ,
                None,
            ),
            Err(MappingError::InvalidAlignment)
        );

        // Zero size rejected
        assert_eq!(
            Mapping::try_new_memory_object(
                test_id(4),
                aspace_id,
                0x0000_6000_0000_0000,
                mem_obj_id,
                0,
                0,
                Rights::READ,
                None,
            ),
            Err(MappingError::ZeroSize)
        );
    }

    #[test]
    fn double_unmap_rejection() {
        let mut mapping = Mapping::try_new_memory_object(
            test_id(1),
            test_id(20),
            0x0000_6000_0000_0000,
            test_id(10),
            0,
            4096,
            Rights::READ,
            None,
        )
        .unwrap();

        // First unmap / close succeeds
        assert!(mapping.close().is_ok());
        assert!(mapping.is_closed());

        // Second unmap / close fails with Closed error (double-unmap rejection)
        assert_eq!(mapping.close(), Err(MappingError::Closed));
    }
}
