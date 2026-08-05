#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref, clippy::undocumented_unsafe_blocks)]
#![cfg_attr(not(test), no_main)]
#![cfg_attr(test, allow(unused_imports, dead_code))]

extern crate alloc;
use alloc::boxed::Box;
use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(manifest_pointer: *const BootstrapManifest, manifest_length: usize) -> ! {
    let manifest = match unsafe {
        libgaxera::entry::initialize_userspace_allocator(
            manifest_pointer,
            manifest_length,
            &ALLOCATOR,
        )
    } {
        Ok(manifest) => manifest,
        Err(_) => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let factory = match manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| entry.role == BootstrapRole::HeapFactory as u16)
        .map(|entry| entry.handle)
    {
        Some(factory) => factory,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let console = match syscall::factory_create(factory, gaxera_abi::ObjectType::DebugConsole) {
        Ok(console) => console,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let value = Box::new(0x51_5253_u64);
    if *value != 0x51_5253_u64 {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    drop(value);
    let _ = syscall::debug_console_write(console, "GAXERA: SERVICE_ALLOCATOR_MIGRATION_SUCCESS\n");
    syscall::exit(0);
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    libgaxera::syscall::exit(0xDEAD_0000);
}
