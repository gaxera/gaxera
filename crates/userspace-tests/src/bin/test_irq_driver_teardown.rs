#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref, clippy::undocumented_unsafe_blocks)]
#![cfg_attr(not(test), no_main)]

use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{InterruptOp, ObjectType};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

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
    let factory = match find(manifest, BootstrapRole::HeapFactory) {
        Some(factory) => factory,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(console) => console,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let irq = match syscall::factory_create_interrupt(factory, 1) {
        Ok(irq) => irq,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let notification = match syscall::factory_create(factory, ObjectType::Notification) {
        Ok(notification) => notification,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    if syscall::interrupt_control_with_arg(irq, InterruptOp::BindNotification, notification.raw())
        .is_err()
    {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    if syscall::interrupt_control(irq, InterruptOp::Unmask).is_err() {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    // Normal capability deletion is the driver-exit path. It must mask and
    // release the vector before a replacement can claim the same IRQ.
    if syscall::delete_handle(irq).is_err() {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    let replacement = match syscall::factory_create_interrupt(factory, 1) {
        Ok(replacement) => replacement,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let _ = syscall::delete_handle(replacement);
    let _ = syscall::delete_handle(notification);
    let _ = syscall::debug_console_write(console, "GAXERA: IRQ_DRIVER_TEARDOWN_SUCCESS\n");
    syscall::exit(0);
}

#[cfg(not(test))]
fn find(manifest: &BootstrapManifest, role: BootstrapRole) -> Option<gaxera_abi::Handle> {
    manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| entry.role == role as u16)
        .map(|entry| entry.handle)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::exit(0xDEAD_0000);
}
