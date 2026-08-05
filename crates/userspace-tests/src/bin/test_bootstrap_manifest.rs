#![no_std]
#![cfg_attr(not(test), no_main)]

use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{Handle, ObjectType};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn _start(manifest_pointer: *const BootstrapManifest, manifest_length: usize) -> ! {
    // SAFETY: Bootloader supplies valid manifest pointer and length.
    if unsafe {
        libgaxera::entry::initialize_userspace_allocator(
            manifest_pointer,
            manifest_length,
            &ALLOCATOR,
        )
    }
    .is_err()
    {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }

    // SAFETY: Bootloader supplies valid manifest pointer and length.
    let manifest =
        match unsafe { libgaxera::entry::bootstrap_manifest(manifest_pointer, manifest_length) } {
            Ok(m) => m,
            Err(_) => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
        };

    let factory = match manifest_capability(manifest, BootstrapRole::HeapFactory, 0) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };

    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(h) => h,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };

    let _ = syscall::debug_console_write(console, "GAXERA: BOOTSTRAP_MANIFEST_SUCCESS\n");
    syscall::exit(0);
}

fn manifest_capability(
    manifest: &BootstrapManifest,
    role: BootstrapRole,
    ordinal: usize,
) -> Option<Handle> {
    let mut match_index = 0usize;
    for entry in &manifest.entries[..usize::from(manifest.entry_count)] {
        if entry.role == role as u16 {
            if match_index == ordinal {
                return Some(entry.handle);
            }
            match_index += 1;
        }
    }
    None
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::exit(0xDEAD_0000);
}
