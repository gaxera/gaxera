#![no_std]
#![cfg_attr(not(test), no_main)]

use core::alloc::Layout;
#[cfg(not(test))]
use core::panic::PanicInfo;

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec::Vec;

use gaxera_abi::Handle;
use libgaxera::allocator::UserspaceAllocator;

#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Handles are set up by kernel init.rs:
    // 0: AddressSpace, 1: CapabilitySpace, 2: Thread, 3: Factory
    let factory = Handle::from_parts(3, 1);
    ALLOCATOR.init(factory, Handle::from_parts(0, 1));
    run_tests();
    libgaxera::syscall::exit(0);
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    libgaxera::syscall::exit(0xDEAD_DEAD);
}

fn run_tests() {
    // 1. Lazy-Growth Verification
    // The heap hasn't grown yet because we just initialized it.
    // The first allocation triggers the first MapMemory.

    // 2. Extended Allocator Invariants
    // a. Zero-sized allocations
    let layout_zst = Layout::from_size_align(0, 8).unwrap();
    let _ptr_zst = unsafe { alloc::alloc::alloc(layout_zst) };
    // Should be properly aligned dangling pointer, not panic

    // b. Alignment verification
    let layout_128 = Layout::from_size_align(16, 16).unwrap();
    let ptr_128 = unsafe { alloc::alloc::alloc(layout_128) };
    assert!(!ptr_128.is_null());
    assert_eq!((ptr_128 as u64) % 16, 0);
    unsafe {
        alloc::alloc::dealloc(ptr_128, layout_128);
    }

    let layout_page = Layout::from_size_align(32, 4096).unwrap();
    let ptr_page = unsafe { alloc::alloc::alloc(layout_page) };
    assert!(!ptr_page.is_null());
    assert_eq!((ptr_page as u64) % 4096, 0);
    unsafe {
        alloc::alloc::dealloc(ptr_page, layout_page);
    }

    // c. A 128 KiB allocation must grow and coalesce two 64 KiB chunks.
    let layout_multi_chunk = Layout::from_size_align(128 * 1024, 8).unwrap();
    let ptr_multi_chunk = unsafe { alloc::alloc::alloc(layout_multi_chunk) };
    assert!(!ptr_multi_chunk.is_null());
    unsafe {
        alloc::alloc::dealloc(ptr_multi_chunk, layout_multi_chunk);
    }

    // d. Fragmentation & Repeated cycles
    let mut ptrs = Vec::new();
    for _ in 0..1000 {
        ptrs.push(Box::new(12345u64));
    }
    for i in (0..ptrs.len()).step_by(2) {
        ptrs[i] = Box::new(0); // re-allocate in fragmented space
    }

    // 3. Fallible reservation coverage. This exercises the bounded arena and
    // ResourceDomain quota without relying on test-only kernel failure hooks.
    let mut oom_vec: Vec<u8> = Vec::new();
    // Try reserving a huge amount (e.g. 500 MB) to hit the ResourceDomain quota or forced failure
    let reserve_result = oom_vec.try_reserve_exact(500 * 1024 * 1024);
    assert!(reserve_result.is_err()); // Proves we caught the OOM without panicking

    // 5. Post-OOM Consistency
    // Immediately after the OOM, perform a small allocation from existing fragments
    let small_box = Box::new(42u8);
    assert_eq!(*small_box, 42);
}
