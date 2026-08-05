#![no_std]
#![cfg_attr(
    all(feature = "alloc_error_handler", target_os = "none"),
    feature(alloc_error_handler)
)]
extern crate alloc;

#[cfg(test)]
pub mod abi_tests;
pub mod allocator;
pub mod arch;
pub mod compat {
    pub mod sockets;
}
pub mod driver;
pub mod entry;
pub mod heap;
pub mod net;
pub mod object;
pub mod prelude;
pub mod process;
pub mod service;
pub mod syscall;
pub mod virtio;

#[cfg(all(not(test), feature = "alloc_error_handler", target_os = "none"))]
#[alloc_error_handler]
fn alloc_error_handler(_layout: core::alloc::Layout) -> ! {
    syscall::exit(0xDEAD_0041);
}
