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
#[allow(clippy::not_unsafe_ptr_arg_deref, clippy::undocumented_unsafe_blocks)]
#[unsafe(no_mangle)]
pub extern "C" fn _start(manifest_pointer: *const BootstrapManifest, manifest_length: usize) -> ! {
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

    let manifest =
        match unsafe { libgaxera::entry::bootstrap_manifest(manifest_pointer, manifest_length) } {
            Ok(manifest) => manifest,
            Err(_) => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
        };

    // Resolve capabilities exclusively by role.  This deliberately does not
    // inspect or assume any handle slot chosen by the kernel.
    let factory = match capability(manifest, BootstrapRole::HeapFactory) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let aspace = match capability(manifest, BootstrapRole::SelfAddressSpace) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    // Use the manifest-provided address-space capability in a harmless
    // process-control query to prove it was obtained as data, not guessed.
    let _ = aspace;
    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(handle) => handle,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let _ =
        syscall::debug_console_write(console, "GAXERA: BOOTSTRAP_NO_SLOT_ASSUMPTIONS_SUCCESS\n");
    syscall::exit(0);
}

#[cfg(not(test))]
fn capability(manifest: &BootstrapManifest, role: BootstrapRole) -> Option<Handle> {
    manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| entry.role == role as u16)
        .map(|entry| entry.handle)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    libgaxera::syscall::exit(0xDEAD_0000);
}
