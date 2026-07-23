use crate::object::ObjectId;

pub trait ArchAddressSpace: Sized {
    type Error;

    /// Returns a physical token representing this address space (e.g. CR3 on x86_64).
    fn root_token(&self) -> u64;

    /// Maps a series of physical frames starting at `virtual_address`.
    fn map_frames(
        &mut self,
        virtual_address: u64,
        frames: &[u64],
        flags: gaxera_abi::Rights,
    ) -> Result<(), Self::Error>;

    /// Maps a range of physical memory (e.g. MMIO) starting at `virtual_address`.
    fn map_physical_range(
        &mut self,
        virtual_address: u64,
        phys_start: u64,
        size: usize,
        rights: gaxera_abi::Rights,
        cache_policy: gaxera_abi::CachePolicy,
    ) -> Result<(), Self::Error>;

    /// Unmaps a range of `page_count` pages starting at `virtual_address`.
    fn unmap_range(&mut self, virtual_address: u64, page_count: usize) -> Result<(), Self::Error>;
}

/// A capability representing a hardware virtual address space (e.g., page tables).
///
/// In this microkernel architecture, `AddressSpace` tracking within the core
/// is minimal. It provides an identity that can be mapped to physical architecture
/// structures in the architecture-specific layers, and mediates memory mapping
/// operations.
/// A capability representing a hardware virtual address space (e.g., page tables).
///
/// In this microkernel architecture, `AddressSpace` tracking within the core
/// is minimal. It provides an identity that can be mapped to physical architecture
/// structures in the architecture-specific layers, and mediates memory mapping
/// operations.
#[derive(Debug, Eq, PartialEq)]
pub struct AddressSpace<A: ArchAddressSpace> {
    id: ObjectId,
    pub arch: A,
}

impl<A: ArchAddressSpace> AddressSpace<A> {
    pub fn new(id: ObjectId, arch: A) -> Self {
        Self { id, arch }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn token(&self) -> AddressSpaceToken {
        AddressSpaceToken::new(self.id, self.arch.root_token())
    }
}

/// A non-owning token representing an address space root (e.g. CR3) without destruction capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AddressSpaceToken {
    id: ObjectId,
    root_token: u64,
}

impl AddressSpaceToken {
    pub fn new(id: ObjectId, root_token: u64) -> Self {
        Self { id, root_token }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn root_token(&self) -> u64 {
        self.root_token
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockArchSpace(u64);
    impl ArchAddressSpace for MockArchSpace {
        type Error = ();
        fn root_token(&self) -> u64 {
            self.0
        }
        fn map_frames(
            &mut self,
            _vaddr: u64,
            _frames: &[u64],
            _rights: gaxera_abi::Rights,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
        fn map_physical_range(
            &mut self,
            _vaddr: u64,
            _phys_start: u64,
            _size: usize,
            _rights: gaxera_abi::Rights,
            _cache_policy: gaxera_abi::CachePolicy,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
        fn unmap_range(&mut self, _vaddr: u64, _pages: usize) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    #[test]
    fn address_space_move_only_and_non_owning_token() {
        let space = AddressSpace::new(ObjectId::from_raw(1), MockArchSpace(0x1000));
        let token1 = space.token();
        let token2 = token1; // Copyable token
        assert_eq!(token1.root_token(), 0x1000);
        assert_eq!(token2.root_token(), 0x1000);
        // `space` cannot be cloned; ownership remains move-only.
        let moved_space = space;
        assert_eq!(moved_space.id().raw(), 1);
    }
}
