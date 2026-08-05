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
    handle: u64,
    opcode: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
) -> u64 {
    let ret: u64;
    // SAFETY: Assembly syscall invocation adhering to x86_64 SysV Gaxera ABI registers.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") 10u64 => ret, // 10 is sys_invoke
            in("rdi") handle,
            in("rsi") opcode,
            in("rdx") arg1,
            in("r10") arg2,
            in("r8") arg3,
            in("r9") arg4,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    ret
}

/// Execute x86_64 raw assembly syscall returning both rax (status) and rdx (value).
///
/// # Safety
/// Caller must ensure that arguments adhere to kernel ABI register constraints.
#[inline(always)]
pub unsafe fn raw_syscall6_ret2(
    handle: u64,
    opcode: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
) -> (u64, u64) {
    let ret_status: u64;
    let ret_val: u64;
    // SAFETY: Assembly syscall invocation adhering to x86_64 SysV Gaxera ABI registers.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") 10u64 => ret_status, // 10 is sys_invoke
            inlateout("rdx") arg1 => ret_val,
            in("rdi") handle,
            in("rsi") opcode,
            in("r10") arg2,
            in("r8") arg3,
            in("r9") arg4,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    (ret_status, ret_val)
}

/// Execute a syscall returning status in RAX and two values in RDX/R10.
///
/// # Safety
/// Caller must ensure that arguments adhere to the Gaxera syscall ABI.
#[inline(always)]
pub unsafe fn raw_syscall6_ret3(
    handle: u64,
    opcode: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
) -> (u64, u64, u64) {
    let ret_status: u64;
    let ret_val: u64;
    let ret_aux: u64;
    // SAFETY: Assembly syscall invocation adhering to x86_64 SysV Gaxera ABI registers.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") 10u64 => ret_status,
            inlateout("rdx") arg1 => ret_val,
            inlateout("r10") arg2 => ret_aux,
            in("rdi") handle,
            in("rsi") opcode,
            in("r8") arg3,
            in("r9") arg4,
            out("rcx") _,
            out("r11") _,
            options(nostack, preserves_flags)
        );
    }
    (ret_status, ret_val, ret_aux)
}
