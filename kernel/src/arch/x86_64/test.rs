#[cfg(any(
    all(feature = "test-breakpoint", feature = "test-divide-error"),
    all(feature = "test-breakpoint", feature = "test-invalid-opcode"),
    all(feature = "test-breakpoint", feature = "test-general-protection"),
    all(feature = "test-breakpoint", feature = "test-page-fault"),
    all(feature = "test-breakpoint", feature = "test-double-fault"),
    all(feature = "test-divide-error", feature = "test-invalid-opcode"),
    all(feature = "test-divide-error", feature = "test-general-protection"),
    all(feature = "test-divide-error", feature = "test-page-fault"),
    all(feature = "test-divide-error", feature = "test-double-fault"),
    all(feature = "test-invalid-opcode", feature = "test-general-protection"),
    all(feature = "test-invalid-opcode", feature = "test-page-fault"),
    all(feature = "test-invalid-opcode", feature = "test-double-fault"),
    all(feature = "test-general-protection", feature = "test-page-fault"),
    all(feature = "test-general-protection", feature = "test-double-fault"),
    all(feature = "test-page-fault", feature = "test-double-fault"),
))]
compile_error!("exactly one Phase 3 exception test feature may be enabled");

#[cfg(any(
    feature = "test-breakpoint",
    feature = "test-divide-error",
    feature = "test-invalid-opcode",
    feature = "test-general-protection",
    feature = "test-page-fault",
    feature = "test-double-fault",
))]
use core::arch::asm;

#[cfg(feature = "test-breakpoint")]
fn run_breakpoint() -> ! {
    use crate::println;

    // SAFETY: This test image deliberately invokes the installed breakpoint
    // gate. The handler returns through `iretq`, proving the resumable path.
    unsafe {
        asm!("int3", options(nomem, nostack));
    }
    println!("GAXERA: EXCEPTION_BREAKPOINT_RESUMED");
    // SAFETY: this function is compiled only with the qemu-test feature.
    unsafe { crate::arch::x86_64::qemu::exit_success() }
}

#[cfg(feature = "test-divide-error")]
fn run_divide_error() -> ! {
    // SAFETY: `div rcx` with a zero divisor generates #DE before any quotient
    // is written. The installed terminal handler owns the resulting path.
    unsafe {
        asm!(
            "xor rdx, rdx",
            "xor rax, rax",
            "xor rcx, rcx",
            "div rcx",
            options(noreturn)
        );
    }
}

#[cfg(feature = "test-invalid-opcode")]
fn run_invalid_opcode() -> ! {
    // SAFETY: `ud2` is architecturally guaranteed to raise #UD.
    unsafe {
        asm!("ud2", options(noreturn));
    }
}

#[cfg(feature = "test-general-protection")]
fn run_general_protection() -> ! {
    // SAFETY: Selector 0xffff is outside the GDT limit. Loading it into DS
    // raises #GP before DS changes, so the handler retains valid data-segment
    // state. This is deliberately not an `int` to a missing IDT gate: that
    // path raises #NP instead of the #GP this test is meant to exercise.
    unsafe {
        asm!("mov ax, 0xffff", "mov ds, ax", options(noreturn));
    }
}

#[cfg(feature = "test-page-fault")]
fn run_page_fault() -> ! {
    // SAFETY: This canonical low-half address is outside every Limine mapping
    // used by Gaxera's early boot image. Dereferencing it deliberately raises
    // #PF and preserves the address in CR2 for the handler report.
    unsafe {
        asm!(
            "mov rax, 0x0000400000000000",
            "mov rax, [rax]",
            options(noreturn)
        );
    }
}

#[cfg(feature = "test-double-fault")]
fn run_double_fault() -> ! {
    // SAFETY: The test-only IDT deliberately leaves the #PF gate non-present.
    // This access first raises #PF; exception delivery then raises #NP for the
    // absent gate. The processor escalates that pair to a genuine #DF, which
    // must enter the configured double-fault IST handler. No `int $8` is used.
    unsafe {
        asm!(
            "mov rax, 0x0000400000000000",
            "mov rax, [rax]",
            options(noreturn)
        );
    }
}

#[cfg(any(
    feature = "test-breakpoint",
    feature = "test-divide-error",
    feature = "test-invalid-opcode",
    feature = "test-general-protection",
    feature = "test-page-fault",
    feature = "test-double-fault",
))]
#[allow(unreachable_patterns)]
pub fn run() -> ! {
    match () {
        #[cfg(feature = "test-breakpoint")]
        () => run_breakpoint(),
        #[cfg(feature = "test-divide-error")]
        () => run_divide_error(),
        #[cfg(feature = "test-invalid-opcode")]
        () => run_invalid_opcode(),
        #[cfg(feature = "test-general-protection")]
        () => run_general_protection(),
        #[cfg(feature = "test-page-fault")]
        () => run_page_fault(),
        #[cfg(feature = "test-double-fault")]
        () => run_double_fault(),
    }
}
