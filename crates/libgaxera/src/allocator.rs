use core::alloc::{GlobalAlloc, Layout};

/// Freestanding Ring-3 Memory Allocator interface backing `#[global_allocator]`.
pub struct UserspaceAllocator;

unsafe impl GlobalAlloc for UserspaceAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}
