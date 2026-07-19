use core::arch::global_asm;

unsafe extern "C" {
    pub fn timer_interrupt_entry();
}

global_asm!(
    r#"
.global timer_interrupt_entry
.extern timer_preempt_handler
.extern timer_kernel_tick
.type timer_interrupt_entry, @function
timer_interrupt_entry:
    // Check if we came from user mode (CS RPL == 3)
    test qword ptr [rsp + 8], 3
    jz .Lkernel

.Luser:
    swapgs
    push rax
    push rbx
    push rcx
    push rdx
    push rsi
    push rdi
    push rbp
    push r8
    push r9
    push r10
    push r11
    push r12
    push r13
    push r14
    push r15

    // Pass TrapFrame pointer to handler
    mov rdi, rsp
    // Stack is 16-byte aligned here (20 * 8 = 160 bytes)
    call timer_preempt_handler

    pop r15
    pop r14
    pop r13
    pop r12
    pop r11
    pop r10
    pop r9
    pop r8
    pop rbp
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rbx
    pop rax
    swapgs
    iretq

.Lkernel:
    // Push caller-saved registers
    push rax
    push rcx
    push rdx
    push rsi
    push rdi
    push r8
    push r9
    push r10
    push r11

    // Align stack via RBP
    push rbp
    mov rbp, rsp
    and rsp, -16

    call timer_kernel_tick

    // Restore stack and RBP
    mov rsp, rbp
    pop rbp

    // Restore caller-saved registers
    pop r11
    pop r10
    pop r9
    pop r8
    pop rdi
    pop rsi
    pop rdx
    pop rcx
    pop rax
    iretq
"#
);
