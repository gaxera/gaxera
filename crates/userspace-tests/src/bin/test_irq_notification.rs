#![no_std]
#![cfg_attr(not(test), no_main)]

extern crate alloc;

use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{Handle, InterruptOp, ObjectType};
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

    // 1. Create a Notification object for IRQ binding
    let notif_handle = match syscall::factory_create(factory, ObjectType::Notification) {
        Ok(h) => h,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(h) => h,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let _ = syscall::debug_console_write(console, "GAXERA: IRQ_TEST_CONSOLE_READY\n");

    // The lower-layer delivery profile must receive an explicitly provisioned IRQ1
    // capability. Silently passing when no capability is present would turn
    // this into a binding-only smoke test and hide a bootstrap failure.
    let irq_handle = match manifest_capability(manifest, BootstrapRole::InterruptObject, 0) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    // 3. Test binding Notification to InterruptObject
    let bind_res = syscall::interrupt_control_with_arg(
        irq_handle,
        InterruptOp::BindNotification,
        notif_handle.raw(),
    );
    if bind_res.is_err() {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    let _ = syscall::debug_console_write(console, "GAXERA: IRQ_TEST_BOUND\n");

    // 4. Unmask and wait for real IRQ1 deliveries. The QEMU runner injects
    // keyboard input through the QMP monitor, exercising the IOAPIC vector,
    // ISR signal, and Ring-3 notification wait.
    if syscall::interrupt_control(irq_handle, InterruptOp::Unmask).is_err() {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    let _ = syscall::debug_console_write(console, "GAXERA: IRQ_TEST_UNMASKED\n");
    if syscall::wait_notification(notif_handle).is_err() {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    let _ = syscall::debug_console_write(console, "GAXERA: IRQ_TEST_FIRST_WAIT\n");
    if syscall::interrupt_control(irq_handle, InterruptOp::Ack).is_err() {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    let _ = syscall::debug_console_write(console, "GAXERA: IRQ_TEST_BEFORE_SECOND_WAIT\n");
    if syscall::wait_notification(notif_handle).is_err() {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    let _ = syscall::debug_console_write(console, "GAXERA: IRQ_TEST_SECOND_WAIT\n");
    if syscall::interrupt_control(irq_handle, InterruptOp::Ack).is_err() {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }

    // 5. Test invalid handle rejection for bind
    let bogus_handle = Handle::from_parts(999, 1);
    let rej_res = syscall::interrupt_control_with_arg(
        irq_handle,
        InterruptOp::BindNotification,
        bogus_handle.raw(),
    );
    assert!(rej_res.is_err());

    if syscall::interrupt_control(irq_handle, InterruptOp::Mask).is_err() {
        syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }

    let _ = syscall::debug_console_write(console, "GAXERA: IRQ_NOTIFICATION_SUCCESS\n");
    let _ = syscall::delete_handle(irq_handle);

    let _ = syscall::delete_handle(notif_handle);
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
    syscall::exit(0xDEAD_0060);
}
