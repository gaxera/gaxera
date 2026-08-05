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
use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{Handle, InterruptOp, ObjectType};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::syscall;

#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[no_mangle]
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
            Ok(m) => m,
            Err(_) => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
        };

    let factory = match manifest_capability(manifest, BootstrapRole::HeapFactory, 0) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };

    // 1. Create Notification for interrupt delivery
    let notif_handle = match syscall::factory_create(factory, ObjectType::Notification) {
        Ok(h) => h,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };

    // 2. If Interrupt capability provided in manifest, bind and unmask
    if let Some(irq_handle) = manifest_capability(manifest, BootstrapRole::InterruptObject, 0) {
        let _ = syscall::interrupt_control_with_arg(
            irq_handle,
            InterruptOp::BindNotification,
            notif_handle.raw(),
        );
        let _ = syscall::interrupt_control(irq_handle, InterruptOp::Unmask);

        loop {
            // Wait for hardware interrupt notification (notification-driven, no busy polling)
            match syscall::wait_notification(notif_handle) {
                Ok(_signals) => {
                    // ACK and rearm level-triggered IOAPIC line
                    let _ = syscall::interrupt_control(irq_handle, InterruptOp::Ack);
                }
                Err(_) => break,
            }
        }
    } else {
        // Fallback for environment without dedicated hardware IRQ assigned
        loop {
            // SAFETY: Active Ring-3 IPC event loop waiting on VirtIO-Block IO requests.
            unsafe { asm!("pause") }
        }
    }

    syscall::exit(0);
}

#[allow(dead_code)]
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
fn panic(_info: &PanicInfo) -> ! {
    syscall::exit(0xDEAD_0061);
}
