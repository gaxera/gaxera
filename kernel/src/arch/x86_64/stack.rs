use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::VirtAddr;
use x86_64::structures::paging::{FrameAllocator, Size4KiB};

use crate::arch::x86_64::paging::KernelPageTables;
use crate::memory::physical::PAGE_SIZE;

/// The base virtual address for dynamically allocated per-thread kernel stacks.
const KERNEL_STACK_BASE: u64 = 0xffff_fe10_0000_0000;
const STACK_SIZE_PAGES: u64 = 4;
const GUARD_SIZE_PAGES: u64 = 1;
const STRIDE_PAGES: u64 = STACK_SIZE_PAGES + GUARD_SIZE_PAGES;

static NEXT_STACK_INDEX: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, PartialEq, Eq)]
pub enum StackError {
    AddressSpaceExhausted,
    AllocationFailed,
    MappingFailed,
}

/// A dynamically allocated per-thread kernel stack.
///
/// The stack is bounded below by an unmapped guard page.
#[allow(dead_code)]
pub struct KernelStack {
    top: VirtAddr,
    base: VirtAddr,
    frames: Vec<x86_64::structures::paging::PhysFrame>,
}

impl KernelStack {
    /// Allocates and maps a new kernel stack.
    #[allow(dead_code)]
    pub fn allocate(
        mapper: &mut KernelPageTables,
        allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> Result<Self, StackError> {
        let index = NEXT_STACK_INDEX.fetch_add(1, Ordering::SeqCst);
        let virt_base = KERNEL_STACK_BASE
            .checked_add(index * STRIDE_PAGES * PAGE_SIZE)
            .ok_or(StackError::AddressSpaceExhausted)?;

        // Virtual base of the mapped stack (skipping the guard page)
        let stack_base = virt_base
            .checked_add(GUARD_SIZE_PAGES * PAGE_SIZE)
            .ok_or(StackError::AddressSpaceExhausted)?;
        let stack_top = stack_base
            .checked_add(STACK_SIZE_PAGES * PAGE_SIZE)
            .ok_or(StackError::AddressSpaceExhausted)?;

        let mut frames = Vec::new();
        frames
            .try_reserve_exact(STACK_SIZE_PAGES as usize)
            .map_err(|_| StackError::AllocationFailed)?;

        for page_idx in 0..STACK_SIZE_PAGES {
            let frame = allocator
                .allocate_frame()
                .ok_or(StackError::AllocationFailed)?;
            frames.push(frame);

            let page_virt = stack_base + (page_idx * PAGE_SIZE);

            // SAFETY: Hardware invariant or verified by caller.
            unsafe {
                mapper
                    .map_kernel_stack_page(page_virt, frame, allocator)
                    .map_err(|_| StackError::MappingFailed)?;
            }
        }

        Ok(Self {
            base: VirtAddr::new(stack_base),
            top: VirtAddr::new(stack_top),
            frames,
        })
    }

    /// Returns the top of the kernel stack (highest address, initial RSP).
    pub fn top(&self) -> VirtAddr {
        self.top
    }
}

// M3.1 limitation: Drop does not reclaim kernel stack memory.
//
// The kernel currently lacks an unmap API in the Mapper, and the physical frame
// allocator does not support returning individual frames. This is acceptable
// for BSP-only cooperative scheduling where thread count is bounded and stacks
// are allocated once. M4/M5 must address stack reclamation when thread
// destruction and SMP are introduced.
impl Drop for KernelStack {
    fn drop(&mut self) {
        // Intentionally empty: see M3.1 limitation above.
    }
}
