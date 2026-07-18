use crate::arch::x86_64::cpu::{UserCopyRecovery, get_cpu_local};
use core::arch::asm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserCopyError {
    InvalidPointer,
    Fault,
}

/// Safely copies bytes from untrusted user space to kernel memory.
pub fn copy_from_user(dst: &mut [u8], src_user_ptr: u64, len: usize) -> Result<(), UserCopyError> {
    if len == 0 {
        return Ok(());
    }

    let end_ptr = src_user_ptr
        .checked_add(len as u64)
        .ok_or(UserCopyError::InvalidPointer)?;
    if src_user_ptr >= 0x0000_8000_0000_0000 || end_ptr > 0x0000_8000_0000_0000 {
        return Err(UserCopyError::InvalidPointer);
    }

    unsafe {
        let cpu_local = get_cpu_local();
        let mut fault_occurred: u64 = 0;

        let recovery_ip: u64;
        asm!(
            "lea {}, [5f]",
            out(reg) recovery_ip,
            options(nomem, nostack, preserves_flags)
        );

        cpu_local.set_recovery(UserCopyRecovery {
            fault_resume_rip: recovery_ip,
        });

        asm!(
            "4:",
            "rep movsb",
            "jmp 6f",
            "5:",
            "mov {}, 1",
            "6:",
            out(reg) fault_occurred,
            inout("rsi") src_user_ptr => _,
            inout("rdi") dst.as_mut_ptr() => _,
            inout("rcx") len => _,
            options(nostack)
        );

        cpu_local.clear_recovery();

        if fault_occurred != 0 {
            Err(UserCopyError::Fault)
        } else {
            Ok(())
        }
    }
}

/// Safely copies bytes from kernel memory to untrusted user space.
pub fn copy_to_user(dst_user_ptr: u64, src: &[u8], len: usize) -> Result<(), UserCopyError> {
    if len == 0 {
        return Ok(());
    }

    let end_ptr = dst_user_ptr
        .checked_add(len as u64)
        .ok_or(UserCopyError::InvalidPointer)?;
    if dst_user_ptr >= 0x0000_8000_0000_0000 || end_ptr > 0x0000_8000_0000_0000 {
        return Err(UserCopyError::InvalidPointer);
    }

    unsafe {
        let cpu_local = get_cpu_local();
        let mut fault_occurred: u64 = 0;

        let recovery_ip: u64;
        asm!(
            "lea {}, [5f]",
            out(reg) recovery_ip,
            options(nomem, nostack, preserves_flags)
        );

        cpu_local.set_recovery(UserCopyRecovery {
            fault_resume_rip: recovery_ip,
        });

        asm!(
            "4:",
            "rep movsb",
            "jmp 6f",
            "5:",
            "mov {}, 1",
            "6:",
            out(reg) fault_occurred,
            inout("rsi") src.as_ptr() => _,
            inout("rdi") dst_user_ptr => _,
            inout("rcx") len => _,
            options(nostack)
        );

        cpu_local.clear_recovery();

        if fault_occurred != 0 {
            Err(UserCopyError::Fault)
        } else {
            Ok(())
        }
    }
}
