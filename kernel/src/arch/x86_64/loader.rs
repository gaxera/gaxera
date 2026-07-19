use crate::arch::x86_64::paging::{KernelPageTables, PagingError};
use crate::memory::mapping::HHDM_BASE;
use crate::memory::physical::{PAGE_SIZE, PhysicalAllocatorError, SegmentedBitmapFrameAllocator};
use kernel_core::elf::parser::ElfParser;
use kernel_core::elf::types::{PF_W, PF_X, PT_LOAD};
use x86_64::structures::paging::{FrameAllocator, PageTableFlags};

#[derive(Clone, Copy, Debug)]
pub enum LoaderError {
    Paging(PagingError),
    Physical(PhysicalAllocatorError),
    WxViolation,
    AddressOverflow,
    OutOfMemory,
}

impl From<PagingError> for LoaderError {
    fn from(err: PagingError) -> Self {
        Self::Paging(err)
    }
}

impl From<PhysicalAllocatorError> for LoaderError {
    fn from(err: PhysicalAllocatorError) -> Self {
        Self::Physical(err)
    }
}

pub fn map_elf_segments(
    page_table: &mut KernelPageTables,
    allocator: &mut SegmentedBitmapFrameAllocator,
    parser: &ElfParser,
) -> Result<(), LoaderError> {
    for segment in parser.program_headers() {
        if segment.p_type != PT_LOAD {
            continue;
        }

        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        let is_writable = (segment.p_flags & PF_W) != 0;
        let is_executable = (segment.p_flags & PF_X) != 0;

        if is_writable && is_executable {
            return Err(LoaderError::WxViolation);
        }

        if is_writable {
            flags |= PageTableFlags::WRITABLE;
        }
        if !is_executable {
            flags |= PageTableFlags::NO_EXECUTE;
        }

        // Calculate page-aligned boundaries
        let start_page = segment.p_vaddr & !(PAGE_SIZE - 1);
        let end_vaddr = segment
            .p_vaddr
            .checked_add(segment.p_memsz)
            .ok_or(LoaderError::AddressOverflow)?;
        let end_page = (end_vaddr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        let mut current_vaddr = start_page;

        while current_vaddr < end_page {
            // Allocate a physical frame for the mapped page
            let phys_frame = allocator.allocate_frame().ok_or(LoaderError::OutOfMemory)?;

            // Map the frame into the page table
            // SAFETY: We checked W^X and we allocate fresh frames from the global allocator.
            unsafe {
                page_table.map_user_page(current_vaddr, phys_frame, flags, allocator)?;
            }

            // Zero the entire frame first (for BSS and padding)
            let frame_ptr = (HHDM_BASE + phys_frame.start_address().as_u64()) as *mut u8;
            unsafe {
                core::ptr::write_bytes(frame_ptr, 0, PAGE_SIZE as usize);
            }

            // If this page overlaps with the segment's file data, copy it
            if current_vaddr < segment.p_vaddr + segment.p_filesz
                && current_vaddr + PAGE_SIZE > segment.p_vaddr
            {
                // Determine the intersection between the page and the file data
                let overlap_start_vaddr = core::cmp::max(current_vaddr, segment.p_vaddr);
                let overlap_end_vaddr = core::cmp::min(
                    current_vaddr + PAGE_SIZE,
                    segment.p_vaddr + segment.p_filesz,
                );
                let overlap_len = (overlap_end_vaddr - overlap_start_vaddr) as usize;

                let file_offset = segment.p_offset + (overlap_start_vaddr - segment.p_vaddr);
                let dest_offset = overlap_start_vaddr - current_vaddr;

                unsafe {
                    core::ptr::copy_nonoverlapping(
                        parser.data().as_ptr().add(file_offset as usize),
                        frame_ptr.add(dest_offset as usize),
                        overlap_len,
                    );
                }
            }

            current_vaddr += PAGE_SIZE;
        }
    }

    Ok(())
}
