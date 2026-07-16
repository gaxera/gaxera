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
