use core::arch::global_asm;

/// The architecture-specific context for cooperative task switching.
///
/// This structure tracks the stack pointer of a suspended thread. The actual
/// register state (callee-saved registers and the return instruction pointer)
/// is stored on the thread's kernel stack.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub rsp: u64,
}

impl Context {
    pub const fn empty() -> Self {
        Self { rsp: 0 }
    }
}

global_asm!(
    r#"
.global context_switch
.type context_switch, @function
context_switch:
    // Arguments:
    // rdi: *mut Context (prev)
    // rsi: *const Context (next)

    // Save callee-saved registers of the previous thread onto its stack.
    // The caller's rip was already pushed by the `call` instruction.
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    // Save the current stack pointer into prev->rsp
    mov [rdi], rsp

    // Load the next thread's stack pointer from next->rsp
    mov rsp, [rsi]

    // Restore the callee-saved registers of the next thread from its stack
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp

    // Return to the next thread. This pops the saved rip from the stack.
    ret
"#
);

unsafe extern "C" {
    /// Switches execution context from `prev` to `next`.
    ///
    /// # Safety
    /// This function switches the stack pointer. The caller must ensure that:
    /// - Both `prev` and `next` point to valid `Context` objects.
    /// - `next.rsp` points to a valid, initialized kernel stack adhering to the
    ///   System V ABI (16-byte alignment before `call`, callee-saved layout).
    /// - No spinlocks or interrupt-disabling locks are held across this call.
    pub fn context_switch(prev: *mut Context, next: *const Context);
}

use crate::arch::x86_64::{cpu, descriptors};
use x86_64::registers::control::{Cr3, Cr3Flags};
use x86_64::structures::paging::PhysFrame;

/// Switches execution context to a new thread, preserving architectural invariants.
///
/// # Safety
/// Must only be called with interrupts disabled to prevent re-entrancy.
/// Both `prev` and `next` must point to valid contexts. `next_kernel_stack_top`
/// must be the top of `next`'s kernel stack. `next_cr3` must be valid if `Some`.
pub unsafe fn switch_thread(
    prev: *mut Context,
    next: *const Context,
    next_kernel_stack_top: u64,
    next_cr3: Option<PhysFrame>,
) {
    // 1. Update CpuLocal's kernel_stack_top (used by syscall entry)
    unsafe { cpu::set_kernel_stack_top(next_kernel_stack_top) };

    // 2. Update TSS.RSP0 (used by hardware interrupt/exception entry from ring 3)
    unsafe { descriptors::set_tss_rsp0(next_kernel_stack_top) };

    // 3. Switch CR3 if the next thread is in a different address space
    if let Some(cr3) = next_cr3 {
        unsafe { Cr3::write(cr3, Cr3Flags::empty()) };
    }

    // 4. Perform the architectural stack and register switch
    unsafe { context_switch(prev, next) };
}
