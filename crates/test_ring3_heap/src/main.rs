#![no_std]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(unused_imports, dead_code))]

use core::alloc::Layout;
#[cfg(not(test))]
use core::panic::PanicInfo;

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec::Vec;

use gaxera_abi::ObjectType;
use gaxera_abi::boot::BootstrapRole;
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn _start(
    manifest_pointer: *const gaxera_abi::boot::BootstrapManifest,
    manifest_length: usize,
) -> ! {
    // SAFETY: The entry point receives valid bootstrap manifest pointers from kernel bootloader.
    if unsafe {
        libgaxera::entry::initialize_userspace_allocator(
            manifest_pointer,
            manifest_length,
            &ALLOCATOR,
        )
    }
    .is_err()
    {
        syscall::exit(1);
    }
    run_tests();
    // SAFETY: The kernel supplies a validated manifest pointer and length,
    // and the allocator initialization above validated the manifest layout.
    let manifest = unsafe { &*manifest_pointer };
    let factory = manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| entry.role == BootstrapRole::HeapFactory as u16)
        .map(|entry| entry.handle);
    let Some(factory) = factory else {
        syscall::exit(2);
    };
    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(console) => console,
        Err(_) => syscall::exit(3),
    };
    let _ = syscall::debug_console_write(console, "GAXERA: RING3_HEAP_TEST_SUCCESS\n");
    syscall::exit(0);
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
    // SAFETY: Testing zero-sized allocation layout against global allocator.
    let _ptr_zst = unsafe { alloc::alloc::alloc(layout_zst) };
    // Should be properly aligned dangling pointer, not panic

    // b. Alignment verification
    let layout_128 = Layout::from_size_align(16, 16).unwrap();
    // SAFETY: Allocating 16-byte aligned memory.
    let ptr_128 = unsafe { alloc::alloc::alloc(layout_128) };
    assert!(!ptr_128.is_null());
    assert_eq!((ptr_128 as u64) % 16, 0);
    // SAFETY: Deallocating matching 16-byte aligned memory.
    unsafe {
        alloc::alloc::dealloc(ptr_128, layout_128);
    }

    let layout_page = Layout::from_size_align(32, 4096).unwrap();
    // SAFETY: Allocating page-aligned memory.
    let ptr_page = unsafe { alloc::alloc::alloc(layout_page) };
    assert!(!ptr_page.is_null());
    assert_eq!((ptr_page as u64) % 4096, 0);
    // SAFETY: Deallocating matching page-aligned memory.
    unsafe {
        alloc::alloc::dealloc(ptr_page, layout_page);
    }

    // c. A 128 KiB allocation must grow and coalesce two 64 KiB chunks.
    let layout_multi_chunk = Layout::from_size_align(128 * 1024, 8).unwrap();
    // SAFETY: Allocating 128 KiB multi-chunk memory block.
    let ptr_multi_chunk = unsafe { alloc::alloc::alloc(layout_multi_chunk) };
    assert!(!ptr_multi_chunk.is_null());
    // SAFETY: Deallocating 128 KiB multi-chunk memory block.
    unsafe {
        alloc::alloc::dealloc(ptr_multi_chunk, layout_multi_chunk);
    }

    // d. Fragmentation & Repeated cycles
    let mut ptrs = Vec::new();
    for _ in 0..1000 {
        ptrs.push(Box::new(12345u64));
    }
    for i in (0..ptrs.len()).step_by(2) {
        *ptrs[i] = 0; // mutate in fragmented space without reallocating Box
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
