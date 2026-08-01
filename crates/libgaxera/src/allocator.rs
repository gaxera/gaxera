use crate::heap::HeapArena;
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use gaxera_abi::Handle;

/// Simple spinlock for the allocator since libgaxera lacks `spin` dependency.
pub struct AllocSpinlock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T> Sync for AllocSpinlock<T> {}

impl<T> AllocSpinlock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> AllocSpinlockGuard<'_, T> {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        AllocSpinlockGuard { lock: self }
    }
}

pub struct AllocSpinlockGuard<'a, T> {
    lock: &'a AllocSpinlock<T>,
}

impl<'a, T> core::ops::Deref for AllocSpinlockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for AllocSpinlockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<'a, T> Drop for AllocSpinlockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

static ARENA: AllocSpinlock<HeapArena> = AllocSpinlock::new(HeapArena::new());

/// Freestanding Ring-3 Memory Allocator interface backing `#[global_allocator]`.
pub struct UserspaceAllocator;

impl UserspaceAllocator {
    /// Initialize the heap with factory and address space capabilities.
    pub fn init(&self, factory: Handle, aspace: Handle) {
        ARENA.lock().init(factory, aspace);
    }

    pub fn teardown(&self) {
        ARENA.lock().teardown();
    }
}

unsafe impl GlobalAlloc for UserspaceAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ARENA.lock().allocate(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        ARENA.lock().deallocate(ptr, layout)
    }
}
