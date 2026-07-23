use crate::arch::x86_64::paging::KernelPageTables;
use crate::memory::physical::SegmentedBitmapFrameAllocator;
use gaxera_abi::Rights;
use kernel_core::address_space::ArchAddressSpace;

#[derive(Debug, Eq, PartialEq)]
pub struct X86AddressSpace {
    pml4_physical_address: u64,
}

impl X86AddressSpace {
    pub fn new(
        page_tables: &mut KernelPageTables,
        physical_allocator: &mut SegmentedBitmapFrameAllocator,
    ) -> Result<Self, &'static str> {
        // For M7, we just use the bootstrap page tables.
        // Wait, the plan was to implement proper AddressSpace ownership!
        // So we should allocate a new Pml4.

        // SAFETY: Hardware invariant or verified by caller.
        let new_pml4 = unsafe { page_tables.fork_for_userspace(physical_allocator) }
            .map_err(|_| "Failed to allocate pml4")?;

        Ok(Self {
            pml4_physical_address: new_pml4,
        })
    }

    pub fn new_dynamic(
        physical_allocator: &mut SegmentedBitmapFrameAllocator,
    ) -> Result<Self, &'static str> {
        // SAFETY: Called post-boot, HHDM is active and valid.
        let new_pml4 = unsafe { KernelPageTables::fork_active_for_userspace(physical_allocator) }
            .map_err(|_| "Failed to allocate pml4")?;
        Ok(Self {
            pml4_physical_address: new_pml4,
        })
    }

    pub fn destroy(
        self,
        allocator: &mut SegmentedBitmapFrameAllocator,
    ) -> Result<(), &'static str> {
        // SAFETY: The pml4 address is valid and no longer active on CPU.
        unsafe {
            KernelPageTables::destroy_user_pml4(self.pml4_physical_address, allocator)
                .map_err(|_| "Failed to destroy user page tables")
        }
    }
}

impl ArchAddressSpace for X86AddressSpace {
    type Error = &'static str;

    fn root_token(&self) -> u64 {
        self.pml4_physical_address
    }

    fn map_frames(
        &mut self,
        virtual_address: u64,
        frames: &[u64],
        rights: Rights,
    ) -> Result<(), Self::Error> {
        let mut phys_alloc_guard = crate::global::PHYSICAL_ALLOCATOR.lock();
        let allocator = phys_alloc_guard
            .as_deref_mut()
            .ok_or("No physical allocator available")?;

        // SAFETY: The pml4 was allocated by fork_for_userspace and is valid.
        unsafe {
            KernelPageTables::map_user_frames(
                self.pml4_physical_address,
                virtual_address,
                frames,
                rights,
                allocator,
            )
            .map_err(|_| "Failed to map user frames")?;
        }
        Ok(())
    }

    fn unmap_range(&mut self, virtual_address: u64, page_count: usize) -> Result<(), Self::Error> {
        // SAFETY: The pml4 physical address is valid.
        unsafe {
            KernelPageTables::unmap_user_range(
                self.pml4_physical_address,
                virtual_address,
                page_count,
            )
            .map_err(|_| "Failed to unmap user range")?;
        }
        Ok(())
    }
}

// Allow cloning so it can be stored in the registry, though sharing the
// raw cr3 might require a refcount later. For now, it's just the physical address.
impl Clone for X86AddressSpace {
    fn clone(&self) -> Self {
        Self {
            pml4_physical_address: self.pml4_physical_address,
        }
    }
}
