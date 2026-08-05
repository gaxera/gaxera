#![no_std]
#![cfg_attr(not(test), no_main)]
#![allow(
    clippy::not_unsafe_ptr_arg_deref,
    clippy::undocumented_unsafe_blocks,
    clippy::while_let_loop,
    unused_imports
)]

#[cfg(not(test))]
use core::arch::asm;
use gaxera_abi::boot::BootstrapManifest;
use libgaxera::allocator::UserspaceAllocator;

#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start(manifest: *const BootstrapManifest, length: usize) -> ! {
    if unsafe { libgaxera::entry::initialize_userspace_allocator(manifest, length, &ALLOCATOR) }
        .is_err()
    {
        libgaxera::syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    let server = resolver_server::DnsResolverServer::new();
    let _ = &server;

    loop {
        // SAFETY: Active Ring-3 IPC event loop waiting on DNS resolution requests.
        unsafe { asm!("pause") }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        // SAFETY: Halting execution.
        unsafe { asm!("pause") }
    }
}
