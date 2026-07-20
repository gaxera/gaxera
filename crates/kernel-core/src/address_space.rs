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
}

/// A capability representing a hardware virtual address space (e.g., page tables).
///
/// In this microkernel architecture, `AddressSpace` tracking within the core
/// is minimal. It provides an identity that can be mapped to physical architecture
/// structures in the architecture-specific layers, and mediates memory mapping
/// operations.
#[derive(Clone, Debug, Eq, PartialEq)]
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
}
