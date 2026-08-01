#![no_std]
#![cfg_attr(not(test), no_main)]

extern crate alloc;

use gaxera_abi::{Handle, Rights};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // Factory is slot 3, AddressSpace is slot 0
    let factory = Handle::from_parts(3, 1);
    let aspace = Handle::from_parts(0, 1);
    ALLOCATOR.init(factory, aspace);

    // 1. Allocate MemoryObject (4096 bytes)
    let mem_handle =
        syscall::factory_create_memory_object(factory, 4096).expect("Failed to allocate memory");

    // 2. Map MemoryObject
    let vaddr: u64 = 0x10000000;
    let rights = Rights::READ | Rights::WRITE;
    let mapping_handle =
        syscall::map_memory(aspace, mem_handle, vaddr, rights).expect("Failed to map memory");

    // 3. Volatile Read/Write
    let ptr = vaddr as *mut u64;
    // SAFETY: The preceding MapMemory syscall installed a writable page at
    // `vaddr`, and the pointer remains within that one-page mapping.
    unsafe {
        core::ptr::write_volatile(ptr, 0xDEADBEEF);
        assert_eq!(core::ptr::read_volatile(ptr), 0xDEADBEEF);
    }

    // 4. Delete Mapping handle (which unmaps it)
    syscall::delete_handle(mapping_handle).expect("Failed to delete mapping");

    // 5. Delete MemoryObject handle (which destroys the object and frees physical frames)
    syscall::delete_handle(mem_handle).expect("Failed to delete memory object");

    // 6. Reallocate to prove quota and frames were returned
    let mem_handle2 =
        syscall::factory_create_memory_object(factory, 4096).expect("Failed to allocate memory 2");

    // 7. Map it again
    let _mapping_handle2 =
        syscall::map_memory(aspace, mem_handle2, vaddr, rights).expect("Failed to map memory 2");

    // 8. Volatile Read/Write again
    // SAFETY: The second MapMemory syscall installed a writable page at the
    // same virtual address after the first mapping was deleted.
    unsafe {
        core::ptr::write_volatile(ptr, 0xCAFEBABE);
        assert_eq!(core::ptr::read_volatile(ptr), 0xCAFEBABE);
    }

    // 9. Clean ExitProcess (handles and remaining memory should be cleaned up automatically)
    syscall::exit(0);
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::exit(1);
}
