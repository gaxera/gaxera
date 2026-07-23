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
}
