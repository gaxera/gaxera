use alloc::vec::Vec;
use core::mem::{align_of, size_of};
use core::ptr::NonNull;

pub const PAGE_SIZE: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SlabError {
    FrameAllocationFailed,
    InvalidPointer,
}

struct SlabPage {
    frame_addr: u64,
    free_head: Option<u16>,
    allocated_count: u16,
    total_slots: u16,
}

/// A typed `SlabCache<T>` manages fixed-size slots of type `T` inside 4 KiB page frames.
///
/// Provides O(1) allocation and deallocation for uniform-sized kernel objects with zero fragmentation.
/// When a slab page becomes empty, its physical frame is returned to the physical frame allocator.
pub struct SlabCache<T> {
    pages: Vec<SlabPage>,
    _marker: core::marker::PhantomData<T>,
}

impl<T> SlabCache<T> {
    pub const fn new() -> Self {
        Self {
            pages: Vec::new(),
            _marker: core::marker::PhantomData,
        }
    }

    pub fn slot_size() -> usize {
        let size = size_of::<T>();
        let align = align_of::<T>();
        let min_size = core::cmp::max(size, size_of::<usize>());
        (min_size + align - 1) & !(align - 1)
    }

    pub fn slots_per_page() -> u16 {
        (PAGE_SIZE / Self::slot_size()) as u16
    }

    pub fn active_page_count(&self) -> usize {
        self.pages.len()
    }

    /// Allocate a slot for type `T` from a slab page.
    ///
    /// `alloc_frame` is a closure that allocates a new physical frame (4 KiB) when needed.
    pub fn allocate<F>(&mut self, mut alloc_frame: F) -> Result<NonNull<T>, SlabError>
    where
        F: FnMut() -> Option<u64>,
    {
        if size_of::<T>() == 0 || Self::slots_per_page() == 0 || Self::slot_size() > PAGE_SIZE {
            return Err(SlabError::InvalidPointer);
        }

        let slot_size = Self::slot_size();
        let total_slots = Self::slots_per_page();

        let page_idx = match self
            .pages
            .iter()
            .position(|p| p.allocated_count < p.total_slots)
        {
            Some(idx) => idx,
            None => {
                let frame_addr = alloc_frame().ok_or(SlabError::FrameAllocationFailed)?;
                let page_ptr = frame_addr as *mut u8;

                for i in 0..total_slots {
                    // SAFETY: Slot offset within allocated 4 KiB frame.
                    let slot_ptr = unsafe { page_ptr.add((i as usize) * slot_size) };
                    let next_idx = if i + 1 < total_slots { i + 1 } else { u16::MAX };
                    // SAFETY: Initializing embedded freelist pointer within slab slot.
                    unsafe {
                        (slot_ptr as *mut u16).write(next_idx);
                    }
                }

                self.pages.push(SlabPage {
                    frame_addr,
                    free_head: Some(0),
                    allocated_count: 0,
                    total_slots,
                });
                self.pages.len() - 1
            }
        };

        let page = &mut self.pages[page_idx];
        let slot_idx = page.free_head.ok_or(SlabError::FrameAllocationFailed)?;

        let page_ptr = page.frame_addr as *mut u8;
        // SAFETY: Slot index calculation within page bounds.
        let slot_ptr = unsafe { page_ptr.add((slot_idx as usize) * slot_size) };

        // SAFETY: Reading next freelist index from initialized slab slot.
        let next_free = unsafe { (slot_ptr as *const u16).read() };
        page.free_head = if next_free == u16::MAX {
            None
        } else {
            Some(next_free)
        };
        page.allocated_count += 1;

        NonNull::new(slot_ptr as *mut T).ok_or(SlabError::FrameAllocationFailed)
    }

    /// Deallocate a slot pointer back to the slab cache.
    ///
    /// `dealloc_frame` is a closure that returns an empty 4 KiB physical frame back to the physical allocator.
    pub fn deallocate<F>(&mut self, ptr: NonNull<T>, mut dealloc_frame: F) -> Result<(), SlabError>
    where
        F: FnMut(u64),
    {
        if size_of::<T>() == 0 || Self::slots_per_page() == 0 || Self::slot_size() > PAGE_SIZE {
            return Err(SlabError::InvalidPointer);
        }

        let ptr_addr = ptr.as_ptr() as u64;
        let frame_addr = ptr_addr & !(PAGE_SIZE as u64 - 1);
        let slot_size = Self::slot_size();

        let page_idx = self
            .pages
            .iter()
            .position(|p| p.frame_addr == frame_addr)
            .ok_or(SlabError::InvalidPointer)?;

        let page = &mut self.pages[page_idx];
        let offset = (ptr_addr - frame_addr) as usize;

        if !offset.is_multiple_of(slot_size) {
            return Err(SlabError::InvalidPointer);
        }

        let slot_idx = (offset / slot_size) as u16;
        if slot_idx >= page.total_slots || page.allocated_count == 0 {
            return Err(SlabError::InvalidPointer);
        }

        // Check for double free by walking free list
        let mut curr = page.free_head;
        let mut steps = 0;
        while let Some(head) = curr {
            if head == slot_idx {
                return Err(SlabError::InvalidPointer);
            }
            if head == u16::MAX || steps >= page.total_slots {
                break;
            }
            // SAFETY: Head slot index is verified within valid slab page bounds.
            let head_ptr = unsafe { (frame_addr as *mut u8).add((head as usize) * slot_size) };
            // SAFETY: Reading freelist index embedded within initialized slot.
            curr = unsafe {
                let next = (head_ptr as *const u16).read();
                if next == u16::MAX { None } else { Some(next) }
            };
            steps += 1;
        }

        let slot_ptr = ptr.as_ptr() as *mut u8;
        let next_idx = page.free_head.unwrap_or(u16::MAX);
        // SAFETY: Writing next freelist index to returned slot.
        unsafe {
            (slot_ptr as *mut u16).write(next_idx);
        }

        page.free_head = Some(slot_idx);
        page.allocated_count -= 1;

        if page.allocated_count == 0 {
            let empty_page = self.pages.remove(page_idx);
            dealloc_frame(empty_page.frame_addr);
        }

        Ok(())
    }
}

impl<T> Default for SlabCache<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    struct TestObject {
        a: u64,
        b: u64,
    }

    #[test]
    fn slab_cache_allocation_and_deallocation() {
        use alloc::alloc::{alloc, dealloc};
        use core::alloc::Layout;
        let mut slab = SlabCache::<TestObject>::new();
        let mut host_pages: Vec<u64> = (0..10)
            .map(|_| {
                // SAFETY: Layout 4096 is non-zero and valid power of two.
                unsafe { alloc(Layout::from_size_align_unchecked(4096, 4096)) as u64 }
            })
            .collect();
        let mut freed_frames = Vec::new();

        let alloc_count = 50;
        let mut ptrs = Vec::new();

        for _ in 0..alloc_count {
            let ptr = slab.allocate(|| host_pages.pop()).unwrap();
            ptrs.push(ptr);
        }

        assert_eq!(slab.active_page_count(), 1);

        for ptr in ptrs {
            slab.deallocate(ptr, |f| freed_frames.push(f)).unwrap();
        }

        assert_eq!(slab.active_page_count(), 0);
        assert_eq!(freed_frames.len(), 1);

        for page in freed_frames {
            // SAFETY: Memory was allocated above with Layout 4096.
            unsafe {
                dealloc(
                    page as *mut u8,
                    Layout::from_size_align_unchecked(4096, 4096),
                );
            }
        }
    }

    #[test]
    fn slab_cache_hardening_validation() {
        use alloc::alloc::{alloc, dealloc};
        use core::alloc::Layout;

        let mut slab = SlabCache::<TestObject>::new();
        // SAFETY: Allocate 4 KiB frame for test
        let frame = unsafe { alloc(Layout::from_size_align_unchecked(4096, 4096)) as u64 };
        let mut frame_opt = Some(frame);

        let ptr = slab.allocate(|| frame_opt.take()).unwrap();

        // 1. Interior pointer rejection
        let interior_ptr = NonNull::new(((ptr.as_ptr() as u64) + 4) as *mut TestObject).unwrap();
        assert_eq!(
            slab.deallocate(interior_ptr, |_| {}),
            Err(SlabError::InvalidPointer)
        );

        // 2. Foreign pointer rejection
        let foreign_ptr = NonNull::new(0x1234_5000 as *mut TestObject).unwrap();
        assert_eq!(
            slab.deallocate(foreign_ptr, |_| {}),
            Err(SlabError::InvalidPointer)
        );

        // 3. Deallocate valid pointer
        assert!(slab.deallocate(ptr, |_| {}).is_ok());

        // 4. Double free rejection
        assert_eq!(slab.deallocate(ptr, |_| {}), Err(SlabError::InvalidPointer));

        // 5. Zero-sized type rejection
        let mut zero_slab = SlabCache::<()>::new();
        assert_eq!(
            zero_slab.allocate(|| Some(frame)),
            Err(SlabError::InvalidPointer)
        );

        // Cleanup host memory
        unsafe {
            dealloc(
                frame as *mut u8,
                Layout::from_size_align_unchecked(4096, 4096),
            );
        }
    }
}
