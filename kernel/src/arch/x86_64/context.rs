use core::arch::global_asm;
#[cfg(feature = "test-context-preservation")]
use core::sync::atomic::{AtomicBool, Ordering};

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

// Sentinel-verified context switch for test-context-preservation.
// Loads known sentinel values into callee-saved registers before calling
// context_switch, then verifies they survived the round-trip. Writes 1
// to CONTEXT_SENTINEL_FLAG if all sentinels match.
#[cfg(feature = "test-context-preservation")]
global_asm!(
    r#"
.global context_switch_verified
.type context_switch_verified, @function
context_switch_verified:
    // Arguments (same as context_switch):
    // rdi: *mut Context (prev)
    // rsi: *const Context (next)

    // Save the caller's real callee-saved registers on the stack.
    push rbp
    push rbx
    push r12
    push r13
    push r14
    push r15

    // Save rdi, rsi across our sentinel load (they are caller-saved per ABI
    // but we need them for the context_switch call).
    push rdi
    push rsi

    // Load distinct sentinel values into all callee-saved registers.
    mov rbx, 0xDEADBEEF00000001
    mov r12, 0xDEADBEEF00000002
    mov r13, 0xDEADBEEF00000003
    mov r14, 0xDEADBEEF00000004
    mov r15, 0xDEADBEEF00000005
    mov rbp, 0xDEADBEEF00000006

    // Restore rdi, rsi for the context_switch call.
    pop rsi
    pop rdi

    // Call context_switch. This will save our sentinel values onto the
    // current stack and switch to the next thread. When this thread is
    // eventually resumed, context_switch will restore the sentinels.
    call context_switch

    // --- Resumed after context switch ---
    // Verify each sentinel survived the round-trip.
    // x86-64 does not support cmp r64, imm64 directly; must load imm64 into a register.
    mov rax, 0xDEADBEEF00000001
    cmp rbx, rax
    jne .sentinel_fail

    mov rax, 0xDEADBEEF00000002
    cmp r12, rax
    jne .sentinel_fail

    mov rax, 0xDEADBEEF00000003
    cmp r13, rax
    jne .sentinel_fail

    mov rax, 0xDEADBEEF00000004
    cmp r14, rax
    jne .sentinel_fail

    mov rax, 0xDEADBEEF00000005
    cmp r15, rax
    jne .sentinel_fail

    mov rax, 0xDEADBEEF00000006
    cmp rbp, rax
    jne .sentinel_fail

    // All sentinels matched — set the passed flag to 1.
    lea rax, [rip + CONTEXT_SENTINEL_FLAG]
    mov byte ptr [rax], 1
    jmp .sentinel_done

.sentinel_fail:
    lea rax, [rip + CONTEXT_SENTINEL_FLAG]
    mov byte ptr [rax], 0

.sentinel_done:
    // Restore the caller's real callee-saved registers.
    pop r15
    pop r14
    pop r13
    pop r12
    pop rbx
    pop rbp
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

#[cfg(feature = "test-context-preservation")]
unsafe extern "C" {
    /// Like `context_switch`, but loads sentinel values into callee-saved
    /// registers before switching and verifies them on resume.
    ///
    /// # Safety
    /// Same requirements as `context_switch`.
    fn context_switch_verified(prev: *mut Context, next: *const Context);
}

/// Atomic flag written by `context_switch_verified` to indicate whether
/// all callee-saved register sentinels survived the context switch.
#[cfg(feature = "test-context-preservation")]
#[unsafe(no_mangle)]
static CONTEXT_SENTINEL_FLAG: AtomicBool = AtomicBool::new(false);

/// Returns true if the sentinel-verified context switch detected that all
/// callee-saved registers were correctly preserved.
#[cfg(feature = "test-context-preservation")]
pub fn context_sentinel_passed() -> bool {
    CONTEXT_SENTINEL_FLAG.load(Ordering::Acquire)
}

// Stub for when the feature is not enabled (avoids cfg noise at call sites).
#[cfg(not(feature = "test-context-preservation"))]
pub fn context_sentinel_passed() -> bool {
    false
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
    #[cfg(feature = "test-context-preservation")]
    unsafe {
        context_switch_verified(prev, next)
    };
    #[cfg(not(feature = "test-context-preservation"))]
    unsafe {
        context_switch(prev, next)
    };
}
