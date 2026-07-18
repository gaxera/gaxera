use crate::arch::x86_64::cpu;
use core::arch::global_asm;
use x86_64::registers::model_specific::{Efer, EferFlags, Msr};
use x86_64::registers::rflags::RFlags;

const MSR_STAR: u32 = 0xC0000081;
const MSR_LSTAR: u32 = 0xC0000082;
const MSR_FMASK: u32 = 0xC0000084;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64, // RFLAGS
    pub r10: u64, // Arg 3 (rcx holds rip, so r10 is arg 3)
    pub r9: u64,  // Arg 5
    pub r8: u64,  // Arg 4
    pub rbp: u64,
    pub rdi: u64, // Arg 0
    pub rsi: u64, // Arg 1
    pub rdx: u64, // Arg 2
    pub rcx: u64, // RIP
    pub rax: u64, // Syscall number
    pub rsp: u64, // User RSP
}

global_asm!(
    r#"
    .global syscall_entry
    .extern handle_syscall
    syscall_entry:
        // 1. Swap GS base to access CpuLocal
        swapgs

        // 2. Save user RSP to scratch space in CpuLocal (offset 8)
        mov gs:[8], rsp

        // 3. Load kernel_stack_top from CpuLocal (offset 0) into RSP
        mov rsp, gs:[0]

        // 4. Construct SyscallFrame on kernel stack
        push gs:[8]      // User RSP
        push rax         // Syscall number
        push rcx         // User RIP
        push rdx         // Arg 2
        push rsi         // Arg 1
        push rdi         // Arg 0
        push rbp
        push r8          // Arg 4
        push r9          // Arg 5
        push r10         // Arg 3
        push r11         // User RFLAGS
        push r12
        push r13
        push r14
        push r15

        // Pass pointer to frame as first arg (&mut SyscallFrame)
        // Align stack to 16-bytes before call (ABI requirement)
        // Currently 15 pushes * 8 bytes = 120 bytes, so rsp is 16n + 8.
        // We push a dummy value (or sub rsp, 8) to make it 16n.
        mov rdi, rsp
        sub rsp, 8
        call handle_syscall
        add rsp, 8

        // Restore registers
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11          // Restore RFLAGS into R11 for sysret
        pop r10
        pop r9
        pop r8
        pop rbp
        pop rdi
        pop rsi
        pop rdx
        pop rcx          // Restore RIP into RCX for sysret
        add rsp, 8       // Skip rax (return value already set or kept)
        pop rsp          // Restore user RSP

        // Swap GS back to user GS
        swapgs
        sysretq
    "#
);

unsafe extern "C" {
    fn syscall_entry();
}

/// Enables x86_64 `syscall`/`sysret` hardware support.
///
/// # Safety
/// Must be called once during early BSP setup.
pub unsafe fn enable_syscalls() {
    unsafe {
        // 1. Enable SCE (System Call Extensions) in EFER
        let current_efer = Efer::read();
        Efer::write(current_efer | EferFlags::SYSTEM_CALL_EXTENSIONS);

        // 2. Program STAR MSR
        // STAR[47:32] = Kernel CS (0x08). SYSRET loads CS = STAR[63:48] + 16 (0x10 + 16 = 0x20 | 3 = 0x23), SS = STAR[63:48] + 8 (0x10 + 8 = 0x18 | 3 = 0x1b)
        let star_val = (0x10_u64 << 48) | (0x08_u64 << 32);
        Msr::new(MSR_STAR).write(star_val);

        // 3. Program LSTAR MSR (syscall entry address)
        let entry_addr = syscall_entry as *const () as usize as u64;
        Msr::new(MSR_LSTAR).write(entry_addr);

        // 4. Program FMASK MSR (mask RFLAGS bits during syscall)
        // Mask IF (Interrupt Flag), TF (Trap Flag), DF (Direction Flag), etc.
        let mask = RFlags::INTERRUPT_FLAG.bits()
            | RFlags::TRAP_FLAG.bits()
            | RFlags::DIRECTION_FLAG.bits();
        Msr::new(MSR_FMASK).write(mask);
    }
}

#[unsafe(no_mangle)]
extern "C" fn handle_syscall(frame: &mut SyscallFrame) {
    // For M2B, handle simple syscalls like NoOp and Yield, or return error for unknown
    match frame.rax {
        0 => {
            // NoOp / Test Syscall
            frame.rax = 0; // Success
        }
        1 => {
            // Yield
            let cpu_local = unsafe { cpu::get_cpu_local() };
            let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };

            if let Some(sched) = scheduler_cell.as_mut()
                && let Some(current_id) = sched.current_thread()
                && let Some(next_id) = sched.dequeue_next()
            {
                unsafe {
                    // Fetch threads
                    let prev_thread = crate::arch::x86_64::thread::THREADS
                        .get_mut(current_id)
                        .unwrap();
                    let _ = sched.enqueue(prev_thread);

                    let next_thread = crate::arch::x86_64::thread::THREADS
                        .get_mut(next_id)
                        .unwrap();
                    let _ = next_thread.make_running();

                    sched.set_current_thread(Some(next_id));

                    let prev_ctx_ptr =
                        &mut prev_thread.arch.context as *mut crate::arch::x86_64::context::Context;

                    let next_thread = crate::arch::x86_64::thread::THREADS
                        .get_mut(next_id)
                        .unwrap();
                    let next_ctx_ptr =
                        &next_thread.arch.context as *const crate::arch::x86_64::context::Context;
                    let next_stack_top = next_thread.arch.stack.top().as_u64();
                    let next_cr3 = next_thread.arch.cr3;

                    crate::arch::x86_64::context::switch_thread(
                        prev_ctx_ptr,
                        next_ctx_ptr,
                        next_stack_top,
                        next_cr3,
                    );
                }
            }
            frame.rax = 0;
        }
        _ => {
            frame.rax = u64::MAX; // Error / Unknown Syscall
        }
    }
}
