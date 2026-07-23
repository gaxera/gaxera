use gaxera_abi::CachePolicy;

use crate::object::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingError {
    InvalidAlignment,
    ZeroSize,
    Closed,
}

/// Pure Range Metadata Mapping Capability (`ObjectType::Mapping = 6`).
///
/// Represents an authorization window over an existing physical address range `[phys_addr, phys_addr + size)`.
/// Stores zero physical frame vector allocations (`Vec<u64>`), guaranteeing fixed-size struct footprint and
/// zero fast-path heap memory allocations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mapping {
    id: ObjectId,
    phys_addr: u64,
    size: usize,
    cache_policy: CachePolicy,
    closed: bool,
}

impl Mapping {
    pub fn try_new(
        id: ObjectId,
        phys_addr: u64,
        size: usize,
        cache_policy: CachePolicy,
    ) -> Result<Self, MappingError> {
        if phys_addr & 0xFFF != 0 {
            return Err(MappingError::InvalidAlignment);
        }
        if size == 0 || (size & 0xFFF) != 0 {
            return Err(MappingError::ZeroSize);
        }
        Ok(Self {
            id,
            phys_addr,
            size,
            cache_policy,
            closed: false,
        })
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn phys_addr(&self) -> u64 {
        self.phys_addr
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn cache_policy(&self) -> CachePolicy {
        self.cache_policy
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn close(&mut self) {
        self.closed = true;
    }

    /// Derive a bounded subregion Mapping from this parent Mapping.
    pub fn derive_subregion(
        &self,
        child_id: ObjectId,
        offset: usize,
        length: usize,
        requested_cache_policy: CachePolicy,
    ) -> Result<Mapping, MappingError> {
        if self.closed {
            return Err(MappingError::Closed);
        }
        if offset & 0xFFF != 0 || length & 0xFFF != 0 {
            return Err(MappingError::InvalidAlignment);
        }
        if length == 0 {
            return Err(MappingError::ZeroSize);
        }

        let parent_start = self.phys_addr;
        let parent_end = parent_start
            .checked_add(self.size as u64)
            .ok_or(MappingError::InvalidAlignment)?;

        let child_start = parent_start
            .checked_add(offset as u64)
            .ok_or(MappingError::InvalidAlignment)?;
        let child_end = child_start
            .checked_add(length as u64)
            .ok_or(MappingError::InvalidAlignment)?;

        if child_start < parent_start || child_end > parent_end {
            return Err(MappingError::InvalidAlignment);
        }

        if requested_cache_policy != self.cache_policy && self.cache_policy == CachePolicy::Uncached
        {
            return Err(MappingError::InvalidAlignment);
        }

        Mapping::try_new(child_id, child_start, length, requested_cache_policy)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn mapping_object_pure_metadata_and_validation() {
        // Valid page-aligned 64 KiB MMIO mapping
        let mapping =
            Mapping::try_new(test_id(1), 0xFEB0_0000, 65536, CachePolicy::Uncached).unwrap();
        assert_eq!(mapping.phys_addr(), 0xFEB0_0000);
        assert_eq!(mapping.size(), 65536);
        assert_eq!(mapping.cache_policy(), CachePolicy::Uncached);

        // Misaligned physical address rejected
        assert_eq!(
            Mapping::try_new(test_id(2), 0xFEB0_0100, 4096, CachePolicy::Uncached),
            Err(MappingError::InvalidAlignment)
        );

        // Misaligned size rejected
        assert_eq!(
            Mapping::try_new(test_id(3), 0xFEB0_0000, 1000, CachePolicy::Uncached),
            Err(MappingError::ZeroSize)
        );
    }

    #[test]
    fn mapping_subregion_derivation_validation() {
        let parent =
            Mapping::try_new(test_id(1), 0xE000_0000, 0x10000, CachePolicy::Uncached).unwrap();

        // Exact boundary derivation (4 KiB at offset 0)
        let child1 = parent
            .derive_subregion(test_id(2), 0, 4096, CachePolicy::Uncached)
            .unwrap();
        assert_eq!(child1.phys_addr(), 0xE000_0000);
        assert_eq!(child1.size(), 4096);

        // Subregion at offset 0x4000
        let child2 = parent
            .derive_subregion(test_id(3), 0x4000, 4096, CachePolicy::Uncached)
            .unwrap();
        assert_eq!(child2.phys_addr(), 0xE000_4000);
        assert_eq!(child2.size(), 4096);

        // Overrun parent boundary (len 0x10000 at offset 0x1000)
        assert_eq!(
            parent.derive_subregion(test_id(4), 0x1000, 0x10000, CachePolicy::Uncached),
            Err(MappingError::InvalidAlignment)
        );

        // Misaligned offset
        assert_eq!(
            parent.derive_subregion(test_id(5), 0x100, 4096, CachePolicy::Uncached),
            Err(MappingError::InvalidAlignment)
        );

        // Closed parent rejection
        let mut closed_parent = parent;
        closed_parent.close();
        assert_eq!(
            closed_parent.derive_subregion(test_id(6), 0, 4096, CachePolicy::Uncached),
            Err(MappingError::Closed)
        );
    }
}
