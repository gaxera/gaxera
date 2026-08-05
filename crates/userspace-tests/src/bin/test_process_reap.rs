#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref, clippy::undocumented_unsafe_blocks)]
#![cfg_attr(not(test), no_main)]

use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

#[path = "../process_support.rs"]
mod process_support;

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(pointer: *const BootstrapManifest, length: usize) -> ! {
    let manifest = match unsafe {
        libgaxera::entry::initialize_userspace_allocator(pointer, length, &ALLOCATOR)
    } {
        Ok(manifest) => manifest,
        Err(_) => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let factory = match process_support::capability(manifest, BootstrapRole::HeapFactory) {
        Some(factory) => factory,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    if process_support::create_terminate_reap(factory, 0x53).is_err() {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    let console = match syscall::factory_create(factory, gaxera_abi::ObjectType::DebugConsole) {
        Ok(console) => console,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let _ = syscall::debug_console_write(console, "GAXERA: PROCESS_REAP_SUCCESS\n");
    syscall::exit(0);
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::exit(0xDEAD_0000);
}
