use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};
use kernel_core::registry::ObjectRegistry;
#[cfg(not(feature = "test-double-fault"))]
use x86_64::registers::control::Cr2;
#[cfg(not(feature = "test-double-fault"))]
use x86_64::structures::idt::PageFaultErrorCode;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

use crate::arch::x86_64::descriptors::{DOUBLE_FAULT_IST_INDEX, StaticCell};
use crate::arch::x86_64::{apic, descriptors, interrupts, ioapic};
#[cfg(feature = "test-heap-guard")]
use crate::memory::mapping::HEAP_LOWER_GUARD;
use crate::println;
#[cfg(not(feature = "qemu-test"))]
use crate::serial;

static IDT: StaticCell<InterruptDescriptorTable> = StaticCell::new(InterruptDescriptorTable::new());
static INITIALIZED: AtomicBool = AtomicBool::new(false);

macro_rules! define_device_irq_handler {
    ($name:ident, $vector:expr) => {
        extern "x86-interrupt" fn $name(_frame: InterruptStackFrame) {
            device_interrupt_handler($vector);
        }
    };
}

define_device_irq_handler!(device_irq_0, interrupts::DEVICE_VECTOR_FIRST);
define_device_irq_handler!(device_irq_1, interrupts::DEVICE_VECTOR_FIRST + 1);
define_device_irq_handler!(device_irq_2, interrupts::DEVICE_VECTOR_FIRST + 2);
define_device_irq_handler!(device_irq_3, interrupts::DEVICE_VECTOR_FIRST + 3);
define_device_irq_handler!(device_irq_4, interrupts::DEVICE_VECTOR_FIRST + 4);
define_device_irq_handler!(device_irq_5, interrupts::DEVICE_VECTOR_FIRST + 5);
define_device_irq_handler!(device_irq_6, interrupts::DEVICE_VECTOR_FIRST + 6);
define_device_irq_handler!(device_irq_7, interrupts::DEVICE_VECTOR_FIRST + 7);
define_device_irq_handler!(device_irq_8, interrupts::DEVICE_VECTOR_FIRST + 8);
define_device_irq_handler!(device_irq_9, interrupts::DEVICE_VECTOR_FIRST + 9);
define_device_irq_handler!(device_irq_10, interrupts::DEVICE_VECTOR_FIRST + 10);
define_device_irq_handler!(device_irq_11, interrupts::DEVICE_VECTOR_FIRST + 11);
define_device_irq_handler!(device_irq_12, interrupts::DEVICE_VECTOR_FIRST + 12);
define_device_irq_handler!(device_irq_13, interrupts::DEVICE_VECTOR_FIRST + 13);
define_device_irq_handler!(device_irq_14, interrupts::DEVICE_VECTOR_FIRST + 14);
define_device_irq_handler!(device_irq_15, interrupts::DEVICE_VECTOR_FIRST + 15);

/// Install the Phase 3 exception handlers.
///
/// # Safety
/// GDT/TSS initialization must already have installed the IST entry used by
/// the double-fault gate. This function must execute once while interrupts are
/// disabled and the IDT storage remains immutable for its active lifetime.
pub unsafe fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        panic!("interrupt descriptor table initialized twice");
    }

    // SAFETY: This is the sole mutable access before the IDT is loaded. The
    // static allocation remains at a fixed address for the kernel lifetime.
    let idt = unsafe { &mut *IDT.get() };
    idt.divide_error.set_handler_fn(divide_error_handler);

    #[cfg(any(feature = "test-user-transition", feature = "test-user-invalid-frame"))]
    {
        idt.breakpoint
            .set_handler_fn(breakpoint_handler)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
    }
    #[cfg(not(any(feature = "test-user-transition", feature = "test-user-invalid-frame")))]
    {
        idt.breakpoint.set_handler_fn(breakpoint_handler);
    }

    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt.general_protection_fault
        .set_handler_fn(general_protection_fault_handler);
    #[cfg(not(feature = "test-double-fault"))]
    idt.page_fault.set_handler_fn(page_fault_handler);
    #[cfg(feature = "test-apic-timer")]
    idt[apic::TIMER_VECTOR].set_handler_fn(local_apic_timer_handler);

    #[cfg(not(feature = "test-apic-timer"))]
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        idt[apic::TIMER_VECTOR].set_handler_addr(x86_64::VirtAddr::new(
            crate::arch::x86_64::timer_entry::timer_interrupt_entry as *const () as usize as u64,
        ));
    }
    idt[apic::SPURIOUS_VECTOR].set_handler_fn(local_apic_spurious_handler);

    idt[interrupts::DEVICE_VECTOR_FIRST].set_handler_fn(device_irq_0);
    idt[interrupts::DEVICE_VECTOR_FIRST + 1].set_handler_fn(device_irq_1);
    idt[interrupts::DEVICE_VECTOR_FIRST + 2].set_handler_fn(device_irq_2);
    idt[interrupts::DEVICE_VECTOR_FIRST + 3].set_handler_fn(device_irq_3);
    idt[interrupts::DEVICE_VECTOR_FIRST + 4].set_handler_fn(device_irq_4);
    idt[interrupts::DEVICE_VECTOR_FIRST + 5].set_handler_fn(device_irq_5);
    idt[interrupts::DEVICE_VECTOR_FIRST + 6].set_handler_fn(device_irq_6);
    idt[interrupts::DEVICE_VECTOR_FIRST + 7].set_handler_fn(device_irq_7);
    idt[interrupts::DEVICE_VECTOR_FIRST + 8].set_handler_fn(device_irq_8);
    idt[interrupts::DEVICE_VECTOR_FIRST + 9].set_handler_fn(device_irq_9);
    idt[interrupts::DEVICE_VECTOR_FIRST + 10].set_handler_fn(device_irq_10);
    idt[interrupts::DEVICE_VECTOR_FIRST + 11].set_handler_fn(device_irq_11);
    idt[interrupts::DEVICE_VECTOR_FIRST + 12].set_handler_fn(device_irq_12);
    idt[interrupts::DEVICE_VECTOR_FIRST + 13].set_handler_fn(device_irq_13);
    idt[interrupts::DEVICE_VECTOR_FIRST + 14].set_handler_fn(device_irq_14);
    idt[interrupts::DEVICE_VECTOR_FIRST + 15].set_handler_fn(device_irq_15);

    #[cfg(any(
        feature = "test-user-transition",
        feature = "test-syscall-round-trip",
        feature = "test-cooperative-yield",
        feature = "test-context-preservation"
    ))]
    {
        // SAFETY: The M2A probe requires a DPL-3 test return gate.
        idt[crate::arch::x86_64::user::USER_RETURN_VECTOR]
            .set_handler_fn(user_return_handler)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
    }

    // SAFETY: IST index 0 names the unique, initialized double-fault stack in
    // the already-loaded TSS. No other Phase 3 IDT gate selects that stack.
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(DOUBLE_FAULT_IST_INDEX);
    }

    // SAFETY: `IDT` is static storage, it is fully initialized above, and this
    // initialization path never mutates it again after loading.
    unsafe {
        idt.load_unsafe();
    }
}

/// Handle a managed legacy IOAPIC vector.  The entry point is deliberately
/// bounded: controller masking, an atomic pending bit, opportunistic
/// notification delivery, and EOI only.  Protocol work remains in Ring 3.
fn device_interrupt_handler(vector: u8) {
    #[cfg(feature = "test-irq-notification")]
    {
        crate::println!("GAXERA: IRQ_ISR vector={:#x}", vector);
        // The diagnostic IRQ profile uses the legacy PS/2 line. Reading its
        // data port acknowledges the controller; the VirtIO driver will own
        // its equivalent device-specific acknowledgement in the later gate.
        if vector == interrupts::DEVICE_VECTOR_FIRST + 1 {
            let mut data = x86_64::instructions::port::Port::<u8>::new(0x60);
            // SAFETY: IRQ1 is the PS/2 controller and the port is valid for
            // this explicitly selected diagnostic profile.
            unsafe {
                let _ = data.read();
            }
        }
    }
    let irq = interrupts::irq_for(vector);
    if let Some(irq) = irq {
        ioapic::ioapic_mask_irq(irq);
    }

    if let Some(record) = interrupts::dispatch(vector) {
        let should_signal = {
            let mut interrupt_objects = crate::global::INTERRUPTS.try_lock();
            interrupt_objects
                .as_deref_mut()
                .and_then(|objects| objects.get_mut(record.interrupt))
                .is_some_and(|object| {
                    matches!(
                        object.begin_delivery(),
                        Ok(Some(notification)) if notification == record.notification
                    )
                })
        };

        if should_signal {
            let wake = {
                let mut notifications = crate::global::NOTIFICATIONS.try_lock();
                notifications
                    .as_deref_mut()
                    .and_then(|registry| registry.get_mut(record.notification))
                    .and_then(|notification| notification.signal(1))
            };
            if let Some(thread_id) = wake {
                crate::arch::x86_64::preemption::wake_from_interrupt(thread_id);
                let _ = interrupts::take_pending(record.lease);
            } else if crate::global::NOTIFICATIONS.is_locked() {
                interrupts::mark_pending(record.lease);
            } else {
                let _ = interrupts::take_pending(record.lease);
            }
        }
    }

    // SAFETY: this vector was delivered by the Local APIC after IOAPIC
    // routing was installed; every path must acknowledge it exactly once.
    unsafe { apic::end_of_interrupt() };
}

#[allow(dead_code)]
extern "x86-interrupt" fn local_apic_timer_handler(_frame: InterruptStackFrame) {
    apic::on_timer_interrupt();
}

extern "x86-interrupt" fn local_apic_spurious_handler(_frame: InterruptStackFrame) {
    // The Local APIC spurious-interrupt path does not require EOI. This gate
    // intentionally does no logging or allocation because it can occur while
    // normal interrupt delivery is active.
}

extern "x86-interrupt" fn breakpoint_handler(frame: InterruptStackFrame) {
    #[cfg(feature = "test-user-transition")]
    {
        let stack_pointer = current_stack_pointer();
        let (start, end) = descriptors::user_transition_stack_bounds();
        if stack_pointer >= start && stack_pointer < end {
            println!("GAXERA: USER_TRANSITION_OK");
            println!(
                "GAXERA: EXCEPTION_BREAKPOINT_CAUGHT ip={:#018x} (user transition)",
                frame.instruction_pointer.as_u64()
            );
            #[cfg(feature = "qemu-test")]
            // SAFETY: Single-core QEMU exit in test mode.
            unsafe {
                crate::arch::x86_64::qemu::exit_success()
            };
            #[cfg(not(feature = "qemu-test"))]
            return;
        }
    }

    println!(
        "GAXERA: EXCEPTION_BREAKPOINT_CAUGHT ip={:#018x}",
        frame.instruction_pointer.as_u64()
    );
}

extern "x86-interrupt" fn divide_error_handler(frame: InterruptStackFrame) {
    fatal_exception("DIVIDE_ERROR", frame, None);
}

extern "x86-interrupt" fn invalid_opcode_handler(frame: InterruptStackFrame) {
    fatal_exception("INVALID_OPCODE", frame, None);
}

extern "x86-interrupt" fn general_protection_fault_handler(
    frame: InterruptStackFrame,
    error_code: u64,
) {
    #[cfg(feature = "test-user-privilege")]
    {
        if frame.code_segment.0 & 0b11 == 0b11 {
            let stack_pointer = current_stack_pointer();
            let (start, end) = descriptors::user_transition_stack_bounds();
            if stack_pointer >= start && stack_pointer < end {
                println!(
                    "GAXERA: USER_PRIVILEGE_DENIED_OK ip={:#018x}",
                    frame.instruction_pointer.as_u64()
                );
                terminal_test_exit();
            }
        }
    }

    fatal_exception("GENERAL_PROTECTION", frame, Some(error_code));
}

#[cfg(not(feature = "test-double-fault"))]
extern "x86-interrupt" fn page_fault_handler(
    #[allow(unused_mut)] mut frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    #[cfg(feature = "test-heap-guard")]
    {
        let _ = frame;
        let accessed = Cr2::read_raw();
        if accessed != HEAP_LOWER_GUARD {
            println!(
                "GAXERA ERROR: HEAP_GUARD_PAGE_FAULT_WRONG_ADDRESS expected={:#018x} actual={:#018x}",
                HEAP_LOWER_GUARD, accessed
            );
            terminal_test_failure();
        }
        println!(
            "GAXERA: HEAP_GUARD_PAGE_FAULT_CAUGHT cr2={:#018x} error={:?}",
            accessed, error_code
        );
        terminal_test_exit();
    }

    #[cfg(not(feature = "test-heap-guard"))]
    {
        let came_from_user = (frame.code_segment.0 & 3) == 3;
        if came_from_user {
            // SAFETY: Hardware invariant or verified by caller.
            unsafe { core::arch::asm!("swapgs", options(nostack, preserves_flags)) };
        }

        // Check if this fault occurred during a recoverable user copy operation.
        // ADR 0009 requires only a matching kernel-mode copy fault to be
        // redirected. User-mode faults or kernel-address faults with an active
        // recovery record indicate a different (possibly serious) fault and
        // must remain terminal.
        // SAFETY: Hardware invariant or verified by caller.
        unsafe {
            let cpu_local = crate::arch::x86_64::cpu::get_cpu_local();
            if let Some(recovery) = cpu_local.take_recovery() {
                let is_kernel_fault = !error_code.contains(PageFaultErrorCode::USER_MODE);
                let fault_addr = Cr2::read_raw();
                let is_user_addr = fault_addr < 0x0000_8000_0000_0000;

                let faulting_ip = frame.instruction_pointer.as_u64();
                let is_copy_instruction = faulting_ip == recovery.faulting_rip;
                let is_copy_range =
                    fault_addr >= recovery.user_start && fault_addr < recovery.user_end;

                if is_kernel_fault && is_user_addr && is_copy_instruction && is_copy_range {
                    // Redirect instruction pointer to recovery landing pad
                    frame.as_mut().update(|val| {
                        val.instruction_pointer = x86_64::VirtAddr::new(recovery.fault_resume_rip);
                    });
                    if came_from_user {
                        core::arch::asm!("swapgs", options(nostack, preserves_flags));
                    }
                    return;
                }
                // Fall through to terminal fault — this is NOT a matching
                // user-copy fault. The recovery record was already consumed
                // by take_recovery() so it cannot apply to a future fault.
            }
        }

        println!(
            "GAXERA: EXCEPTION_PAGE_FAULT_CAUGHT ip={:#018x} cr2={:#018x} error={:?}",
            frame.instruction_pointer.as_u64(),
            Cr2::read_raw(),
            error_code
        );
        if came_from_user {
            // GS is ALREADY kernel GS because we swapped it at the start of page_fault_handler.
            // DO NOT swapgs here!
            // SAFETY: Access to scheduler and threads is safe as interrupts are disabled during exception handling.
            let (current_id, next_id) = unsafe {
                let cpu_local = crate::arch::x86_64::cpu::get_cpu_local();
                let scheduler_cell = &mut *cpu_local.scheduler.get();
                if let Some(scheduler) = scheduler_cell.as_mut() {
                    if let Some(current_id) = scheduler.current_thread() {
                        if let Some(next_id) = scheduler.dequeue_next() {
                            scheduler.set_current_thread(Some(next_id));
                            (Some(current_id), Some(next_id))
                        } else {
                            (Some(current_id), None)
                        }
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                }
            };

            if let (Some(c_id), Some(n_id)) = (current_id, next_id) {
                // SAFETY: We verify the thread ID exists before operating on it.
                unsafe {
                    if let Some(thread) = crate::arch::x86_64::thread::THREADS.get_mut(c_id) {
                        let _ = thread.make_dying();
                        let _ = thread.make_dead();
                    }
                }
                let _ = crate::arch::x86_64::preemption::switch_to_next(c_id, n_id);
                // We never return here since this thread is dead.
                loop {
                    // SAFETY: Halting execution safely.
                    unsafe { core::arch::asm!("pause") }
                }
            } else {
                println!("GAXERA ERROR: No runnable threads left after user thread died.");
                terminal_test_exit();
            }
        }
        terminal_test_exit();
    }
}

extern "x86-interrupt" fn double_fault_handler(frame: InterruptStackFrame, error_code: u64) -> ! {
    let stack_pointer = current_stack_pointer();
    if !descriptors::is_on_double_fault_stack(stack_pointer) {
        println!(
            "GAXERA ERROR: EXCEPTION_DOUBLE_FAULT_IST_STACK_MISMATCH rsp={:#018x}",
            stack_pointer
        );
        terminal_test_failure();
    }

    println!(
        "GAXERA: EXCEPTION_DOUBLE_FAULT_IST_CAUGHT ip={:#018x} error={:#x} rsp={:#018x}",
        frame.instruction_pointer.as_u64(),
        error_code,
        stack_pointer
    );
    terminal_test_exit();
}

fn current_stack_pointer() -> u64 {
    let stack_pointer: u64;
    // SAFETY: Reading RSP has no side effects. The double-fault handler calls
    // this before it emits its success marker, so the test can prove the
    // processor selected the configured IST allocation rather than merely
    // reaching the handler through an accidental stack path.
    unsafe {
        asm!("mov {}, rsp", out(reg) stack_pointer, options(nomem, nostack, preserves_flags));
    }
    stack_pointer
}

fn fatal_exception(name: &str, frame: InterruptStackFrame, error_code: Option<u64>) -> ! {
    match error_code {
        Some(error_code) => println!(
            "GAXERA: EXCEPTION_{name}_CAUGHT ip={:#018x} error={:#x}",
            frame.instruction_pointer.as_u64(),
            error_code
        ),
        None => println!(
            "GAXERA: EXCEPTION_{name}_CAUGHT ip={:#018x}",
            frame.instruction_pointer.as_u64()
        ),
    }
    terminal_test_exit();
}

fn terminal_test_exit() -> ! {
    #[cfg(feature = "qemu-test")]
    {
        // SAFETY: every exception test image is launched by xtask with the
        // matching QEMU isa-debug-exit device attached.
        unsafe { crate::arch::x86_64::qemu::exit_success() }
    }

    #[cfg(not(feature = "qemu-test"))]
    serial::halt()
}

fn terminal_test_failure() -> ! {
    #[cfg(feature = "qemu-test")]
    {
        // SAFETY: every exception test image is launched by xtask with the
        // matching QEMU isa-debug-exit device attached.
        unsafe { crate::arch::x86_64::qemu::exit_failure() }
    }

    #[cfg(not(feature = "qemu-test"))]
    serial::halt()
}

#[cfg(any(
    feature = "test-user-transition",
    feature = "test-syscall-round-trip",
    feature = "test-cooperative-yield",
    feature = "test-context-preservation"
))]
#[allow(unused_variables)]
extern "x86-interrupt" fn user_return_handler(frame: InterruptStackFrame) {
    #[cfg(feature = "test-user-transition")]
    {
        let stack_pointer = current_stack_pointer();
        let (start, end) = descriptors::user_transition_stack_bounds();
        if stack_pointer < start || stack_pointer >= end {
            println!(
                "GAXERA ERROR: USER_RETURN_STACK_MISMATCH rsp={:#018x}",
                stack_pointer
            );
            terminal_test_failure();
        }
    }

    // SAFETY: Hardware invariant or verified by caller.
    unsafe { crate::arch::x86_64::probe::M2AProbe::restore_kernel_cr3() };

    #[cfg(feature = "test-user-transition")]
    println!(
        "GAXERA: USER_TRANSITION_OK ip={:#018x}",
        frame.instruction_pointer.as_u64()
    );

    #[cfg(feature = "test-syscall-round-trip")]
    println!("GAXERA: SYSCALL_ROUND_TRIP_OK");

    #[cfg(feature = "test-cooperative-yield")]
    println!("GAXERA: COOPERATIVE_YIELD_OK");

    #[cfg(feature = "test-context-preservation")]
    {
        if crate::arch::x86_64::context::context_sentinel_passed() {
            println!("GAXERA: CONTEXT_PRESERVATION_OK");
        } else {
            println!("GAXERA ERROR: CONTEXT_PRESERVATION_FAILED");
            terminal_test_failure();
        }
    }
    terminal_test_exit();
}
