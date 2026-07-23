use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

use linked_list_allocator::LockedHeap;

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();
static INITIALIZED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapInitError {
    EmptyRange,
    UnalignedStart,
    UnalignedSize,
    AddressOverflow,
    AlreadyInitialized,
}

impl fmt::Display for HeapInitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRange => f.write_str("heap range is empty"),
            Self::UnalignedStart => f.write_str("heap start is not page aligned"),
            Self::UnalignedSize => f.write_str("heap size is not page aligned"),
            Self::AddressOverflow => f.write_str("heap range wraps virtual address space"),
            Self::AlreadyInitialized => f.write_str("heap initialized twice"),
        }
    }
}

/// Initialize the fixed Phase 4 kernel heap.
///
/// # Safety
/// The caller must map `[heap_start, heap_start + heap_size)` as writable,
/// non-executable memory that is not aliased for another allocator or device.
/// This is a one-time bootstrap operation before concurrent allocation exists.
pub unsafe fn init(heap_start: usize, heap_size: usize) -> Result<(), HeapInitError> {
    if heap_size == 0 {
        return Err(HeapInitError::EmptyRange);
    }
    if !heap_start.is_multiple_of(4096) {
        return Err(HeapInitError::UnalignedStart);
    }
    if !heap_size.is_multiple_of(4096) {
        return Err(HeapInitError::UnalignedSize);
    }
    if heap_start.checked_add(heap_size).is_none() {
        return Err(HeapInitError::AddressOverflow);
    }
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(HeapInitError::AlreadyInitialized);
    }
    // SAFETY: caller establishes the mapped, exclusive heap range invariant.
    unsafe { HEAP.lock().init(heap_start as *mut u8, heap_size) };
    Ok(())
}

/// Dynamically extends the kernel heap by allocating physical frames and mapping them into kernel virtual memory.
///
/// # Safety
/// Caller must ensure `virtual_address` names a page-aligned, unmapped kernel virtual memory range.
pub unsafe fn extend_heap(
    virtual_address: u64,
    frame_count: usize,
    allocator: &mut crate::memory::physical::SegmentedBitmapFrameAllocator<'_>,
    page_tables: &mut crate::arch::x86_64::paging::KernelPageTables,
) -> Result<(), &'static str> {
    let byte_size = frame_count * 4096;
    for i in 0..frame_count {
        let frame = allocator
            .allocate()
            .ok_or("Physical allocation failed during heap extension")?;
        let vaddr = virtual_address + (i * 4096) as u64;
        // SAFETY: Mapping kernel heap extension frames as R/W
        unsafe {
            page_tables
                .map_kernel_page(vaddr, frame, allocator)
                .map_err(|_| "Failed to map kernel heap extension page")?;
        }
    }

    // SAFETY: Newly mapped virtual memory range is exclusive and writable.
    unsafe {
        HEAP.lock().extend(byte_size);
    }
    Ok(())
}
