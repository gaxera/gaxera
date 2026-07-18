use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};
#[cfg(not(feature = "test-double-fault"))]
use x86_64::registers::control::Cr2;
#[cfg(not(feature = "test-double-fault"))]
use x86_64::structures::idt::PageFaultErrorCode;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

use crate::arch::x86_64::descriptors::{DOUBLE_FAULT_IST_INDEX, StaticCell};
use crate::arch::x86_64::{apic, descriptors};
#[cfg(feature = "test-heap-guard")]
use crate::memory::mapping::HEAP_LOWER_GUARD;
use crate::println;
#[cfg(not(feature = "qemu-test"))]
use crate::serial;

static IDT: StaticCell<InterruptDescriptorTable> = StaticCell::new(InterruptDescriptorTable::new());
static INITIALIZED: AtomicBool = AtomicBool::new(false);

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
    
    #[cfg(any(feature = "test-user-transition", feature = "test-user-privilege", feature = "test-user-invalid-frame"))]
    {
        idt.breakpoint
            .set_handler_fn(breakpoint_handler)
            .set_privilege_level(x86_64::PrivilegeLevel::Ring3);
    }
    #[cfg(not(any(feature = "test-user-transition", feature = "test-user-privilege", feature = "test-user-invalid-frame")))]
    {
        idt.breakpoint.set_handler_fn(breakpoint_handler);
    }

    idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);
    idt.general_protection_fault
        .set_handler_fn(general_protection_fault_handler);
    #[cfg(not(feature = "test-double-fault"))]
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt[apic::TIMER_VECTOR].set_handler_fn(local_apic_timer_handler);
    idt[apic::SPURIOUS_VECTOR].set_handler_fn(local_apic_spurious_handler);

    #[cfg(feature = "test-user-transition")]
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
            println!(
                "GAXERA: EXCEPTION_BREAKPOINT_CAUGHT ip={:#018x} (user transition)",
                frame.instruction_pointer.as_u64()
            );
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
    frame: InterruptStackFrame,
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
    println!(
        "GAXERA: EXCEPTION_PAGE_FAULT_CAUGHT ip={:#018x} cr2={:#018x} error={:?}",
        frame.instruction_pointer.as_u64(),
        Cr2::read_raw(),
        error_code
    );
    #[cfg(not(feature = "test-heap-guard"))]
    terminal_test_exit();
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

#[cfg(feature = "test-user-transition")]
extern "x86-interrupt" fn user_return_handler(frame: InterruptStackFrame) {
    let stack_pointer = current_stack_pointer();
    let (start, end) = descriptors::user_transition_stack_bounds();
    if stack_pointer < start || stack_pointer >= end {
        println!(
            "GAXERA ERROR: USER_RETURN_STACK_MISMATCH rsp={:#018x}",
            stack_pointer
        );
        terminal_test_failure();
    }

    unsafe { crate::arch::x86_64::probe::M2AProbe::restore_kernel_cr3() };

    println!(
        "GAXERA: USER_TRANSITION_OK ip={:#018x}",
        frame.instruction_pointer.as_u64()
    );
    terminal_test_exit();
}
