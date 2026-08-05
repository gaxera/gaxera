#![no_std]
#![cfg_attr(not(test), no_main)]

extern crate alloc;

use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{Handle, ObjectType, Rights};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn _start(manifest_pointer: *const BootstrapManifest, manifest_length: usize) -> ! {
    // SAFETY: Bootloader provides valid bootstrap manifest pointers.
    let manifest =
        unsafe { libgaxera::entry::bootstrap_manifest(manifest_pointer, manifest_length) };
    let manifest = match manifest {
        Ok(manifest) => manifest,
        Err(_) => libgaxera::syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let factory = match manifest_capability(manifest, BootstrapRole::HeapFactory, 0) {
        Ok(handle) => handle,
        Err(_) => libgaxera::syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let aspace = match manifest_capability(manifest, BootstrapRole::SelfAddressSpace, 0) {
        Ok(handle) => handle,
        Err(_) => libgaxera::syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    ALLOCATOR.init(factory, aspace);

    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(h) => h,
        Err(_) => syscall::exit(10),
    };
    let _ = syscall::debug_console_write(console, "GAXERA: TEST_MEMORY_LIFECYCLE_STARTED\n");

    // 1. Allocate MemoryObject (4096 bytes)
    let mem_handle = match syscall::factory_create_memory_object(factory, 4096) {
        Ok(handle) => handle,
        Err(_) => {
            let _ = syscall::debug_console_write(console, "GAXERA: FAIL STEP 1\n");
            syscall::exit(11);
        }
    };
    let _ = syscall::debug_console_write(console, "GAXERA: STEP 1 OK\n");

    // 2. Map MemoryObject
    let vaddr: u64 = 0x10000000;
    let rights = Rights::READ | Rights::WRITE;
    let mapping_handle = match syscall::map_memory(aspace, mem_handle, vaddr, rights) {
        Ok(handle) => handle,
        Err(_) => {
            let _ = syscall::debug_console_write(console, "GAXERA: FAIL STEP 2\n");
            syscall::exit(12);
        }
    };
    let _ = syscall::debug_console_write(console, "GAXERA: STEP 2 OK\n");

    // 3. Volatile Read/Write
    let ptr = vaddr as *mut u64;
    // SAFETY: Volatile access to newly mapped valid memory object page.
    unsafe {
        core::ptr::write_volatile(ptr, 0xDEADBEEF);
        if core::ptr::read_volatile(ptr) != 0xDEADBEEF {
            let _ = syscall::debug_console_write(console, "GAXERA: FAIL STEP 3\n");
            syscall::exit(13);
        }
    }
    let _ = syscall::debug_console_write(console, "GAXERA: STEP 3 OK\n");

    // 4. Unmap MemoryObject
    if syscall::unmap_memory(mapping_handle).is_err() {
        let _ = syscall::debug_console_write(console, "GAXERA: FAIL STEP 4\n");
        syscall::exit(14);
    }
    let _ = syscall::debug_console_write(console, "GAXERA: STEP 4 OK\n");

    // 5. Delete MemoryObject handle (which destroys the object and frees physical frames)
    if syscall::delete_handle(mem_handle).is_err() {
        let _ = syscall::debug_console_write(console, "GAXERA: FAIL STEP 5\n");
        syscall::exit(15);
    }
    let _ = syscall::debug_console_write(console, "GAXERA: STEP 5 OK\n");

    // 6. Reallocate to prove quota and frames were returned
    let mem_handle2 = match syscall::factory_create_memory_object(factory, 4096) {
        Ok(handle) => handle,
        Err(_) => {
            let _ = syscall::debug_console_write(console, "GAXERA: FAIL STEP 6\n");
            syscall::exit(16);
        }
    };
    let _ = syscall::debug_console_write(console, "GAXERA: STEP 6 OK\n");

    // 7. Map it again
    let _mapping_handle2 = match syscall::map_memory(aspace, mem_handle2, vaddr, rights) {
        Ok(handle) => handle,
        Err(_) => {
            let _ = syscall::debug_console_write(console, "GAXERA: FAIL STEP 7\n");
            syscall::exit(17);
        }
    };
    let _ = syscall::debug_console_write(console, "GAXERA: STEP 7 OK\n");

    // 8. Volatile Read/Write again
    // SAFETY: Volatile access to re-mapped valid memory object page.
    unsafe {
        core::ptr::write_volatile(ptr, 0xCAFEBABE);
        if core::ptr::read_volatile(ptr) != 0xCAFEBABE {
            let _ = syscall::debug_console_write(console, "GAXERA: FAIL STEP 8\n");
            syscall::exit(18);
        }
    }
    let _ = syscall::debug_console_write(console, "GAXERA: STEP 8 OK\n");

    // 9. Emit test confirmation marker via DebugConsole
    let _ = syscall::debug_console_write(console, "GAXERA: MEMORY_RECLAIMED_AND_QUOTA_REFUNDED\n");

    // 10. Clean ExitProcess
    syscall::exit(0);
}

#[cfg(not(test))]
fn manifest_capability(
    manifest: &BootstrapManifest,
    role: BootstrapRole,
    ordinal: usize,
) -> Result<Handle, ()> {
    let mut match_index = 0usize;
    for entry in &manifest.entries[..usize::from(manifest.entry_count)] {
        if entry.role == role as u16 {
            if match_index == ordinal {
                return Ok(entry.handle);
            }
            match_index += 1;
        }
    }
    Err(())
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::exit(1);
}
