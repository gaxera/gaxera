#![no_std]
#![allow(clippy::not_unsafe_ptr_arg_deref, clippy::undocumented_unsafe_blocks)]
#![cfg_attr(not(test), no_main)]

use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{ObjectType, ProcessControlOp};
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
    let factory = match find(manifest, BootstrapRole::HeapFactory) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(console) => console,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let process = match syscall::create_process(factory, 128, 128, 4 * 1024 * 1024) {
        Ok(process) => process,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    if syscall::process_control(process, ProcessControlOp::Query, 0, 0, 0) != Ok(1) {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    let child_aspace =
        match syscall::process_control(process, ProcessControlOp::AcquireAddressSpace, 0, 0, 0) {
            Ok(raw) => gaxera_abi::Handle::from_raw(raw),
            Err(_) => syscall::exit(gaxera_abi::status::INTERNAL_ERROR),
        };
    let child_thread =
        match syscall::process_control(process, ProcessControlOp::AcquireMainThread, 0, 0, 0) {
            Ok(raw) => gaxera_abi::Handle::from_raw(raw),
            Err(_) => syscall::exit(gaxera_abi::status::INTERNAL_ERROR),
        };
    let _ = syscall::delete_handle(child_aspace);
    let _ = syscall::delete_handle(child_thread);
    if syscall::process_control(process, ProcessControlOp::Terminate, 0x51, 0, 0).is_err()
        || syscall::process_control(process, ProcessControlOp::Query, 0, 0, 0) != Ok(6)
        || syscall::process_control(process, ProcessControlOp::Reap, 0, 0, 0).is_err()
    {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    let _ =
        syscall::debug_console_write(console, "GAXERA: PROCESS_CREATE_TERMINATE_REAP_SUCCESS\n");
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
    libgaxera::syscall::exit(0xDEAD_0000);
}
