use core::arch::asm;

pub const INLINE_IPC_REGISTER_BYTES: usize = 64;

/// Execute x86_64 raw assembly syscall instruction with 6 register parameters.
///
/// Registers:
/// - rax: opcode (in/out: return value)
/// - rdi: arg1 (handle)
/// - rsi: arg2
/// - rdx: arg3
/// - r10: arg4
/// - r8:  arg5
///
/// # Safety
/// Invokes a kernel system call. Register state must match kernel ABI rules.
#[inline(always)]
pub unsafe fn raw_syscall6(
    opcode: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
) -> u64 {
    let ret: u64;
    // SAFETY: Assembly syscall invocation adhering to x86_64 SysV Gaxera ABI registers.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") opcode => ret,
            in("rdi") arg1,
            in("rsi") arg2,
            in("rdx") arg3,
            in("r10") arg4,
            in("r8") arg5,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}
