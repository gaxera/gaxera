use crate::arch::x86_64::cpu;
use core::arch::global_asm;
use x86_64::registers::model_specific::{Efer, EferFlags, Msr};
use x86_64::registers::rflags::RFlags;

use crate::memory::mapping::USER_ADDRESS_MAX;
use crate::println;

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

/// Validates the sysret return frame for safety.
///
/// `sysretq` loads RIP from RCX and RFLAGS from R11. If RCX contains a
/// non-canonical address, the processor raises `#GP(0)` while still at CPL 0
/// (the well-known sysret vulnerability). This function ensures the return
/// frame cannot trigger that condition or restore forbidden RFLAGS bits.
fn validate_sysret_frame(frame: &SyscallFrame) -> bool {
    // RCX (return RIP) and RSP must be non-zero lower-half canonical user
    // addresses. `sysretq` consumes RCX while still at CPL 0; RSP is restored
    // before the privilege transition in the entry assembly, so both fields
    // are part of the kernel return boundary.
    if !is_user_return_address(frame.rcx) || !is_user_return_address(frame.rsp) {
        return false;
    }

    // R11 (return RFLAGS):
    // - Bit 1 (fixed-one) must be set
    // - IF (bit 9) should be set for user mode
    // - IOPL (bits 12:13) must be zero
    // - NT (bit 14) must be clear
    // - VM (bit 17) must be clear
    // - AC (bit 18) must be clear
    let r11 = frame.r11;
    let rflags_fixed_one: u64 = 1 << 1;
    let rflags_forbidden: u64 = (3 << 12) | (1 << 14) | (1 << 17) | (1 << 18);

    if r11 & rflags_fixed_one == 0 {
        return false;
    }
    if r11 & rflags_forbidden != 0 {
        return false;
    }

    true
}

const fn is_user_return_address(address: u64) -> bool {
    address != 0 && address <= USER_ADDRESS_MAX
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
    frame.rax = match frame.rax {
        0 => {
            // NoOp / Test Syscall
            0
        }
        1 => match yield_current_thread() {
            Ok(()) => 0,
            Err(()) => u64::MAX,
        },
        _ => u64::MAX, // Error / unknown syscall
    };

    // Validate the return frame before sysretq executes.
    // A non-canonical RCX would cause #GP(0) at CPL 0 (sysret vulnerability).
    // Forbidden RFLAGS bits in R11 could grant user code IOPL or other
    // dangerous state.
    if !validate_sysret_frame(frame) {
        println!(
            "GAXERA ERROR: SYSRET_VALIDATION_FAILED rcx={:#018x} r11={:#018x}",
            frame.rcx, frame.r11
        );
        #[cfg(feature = "qemu-test")]
        unsafe {
            crate::arch::x86_64::qemu::exit_failure();
        }
        #[cfg(not(feature = "qemu-test"))]
        crate::serial::halt();
    }
}

fn yield_current_thread() -> Result<(), ()> {
    let cpu_local = unsafe { cpu::get_cpu_local() };
    let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
    let scheduler = scheduler_cell.as_mut().ok_or(())?;
    let current_id = scheduler.current_thread().ok_or(())?;
    let next_id = match scheduler.next_runnable() {
        Some(id) => id,
        None => return Ok(()),
    };

    unsafe {
        crate::arch::x86_64::thread::THREADS
            .with_two_mut(current_id, next_id, |current, next| {
                if current.state() != kernel_core::thread::ThreadState::Running
                    || next.state() != kernel_core::thread::ThreadState::Runnable
                {
                    return Err(());
                }

                scheduler
                    .commit_yield(current_id, next_id)
                    .map_err(|_| ())?;
                current.make_runnable().map_err(|_| ())?;
                next.make_running().map_err(|_| ())?;

                let current_context = &mut current.arch.context as *mut _;
                let next_context = &next.arch.context as *const _;
                let next_stack_top = next.arch.stack.top().as_u64();
                let next_cr3 = next.arch.cr3;

                // SAFETY: queue and thread state are committed as one BSP-only
                // transition; both contexts and the incoming stack are live.
                crate::arch::x86_64::context::switch_thread(
                    current_context,
                    next_context,
                    next_stack_top,
                    next_cr3,
                );
                Ok(())
            })
            .ok_or(())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_frame() -> SyscallFrame {
        SyscallFrame {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 1 << 1,
            r10: 0,
            r9: 0,
            r8: 0,
            rbp: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            rcx: 0x1000,
            rax: 0,
            rsp: 0x2000,
        }
    }

    #[test]
    fn sysret_validation_rejects_hostile_return_addresses_and_flags() {
        let frame = valid_frame();
        assert!(validate_sysret_frame(&frame));
        assert!(!validate_sysret_frame(&SyscallFrame { rcx: 0, ..frame }));
        assert!(!validate_sysret_frame(&SyscallFrame {
            rsp: USER_ADDRESS_MAX + 1,
            ..frame
        }));
        assert!(!validate_sysret_frame(&SyscallFrame {
            r11: (1 << 1) | (3 << 12),
            ..frame
        }));
    }
}
