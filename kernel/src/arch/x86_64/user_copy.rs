use crate::arch::x86_64::cpu::{UserCopyRecovery, get_cpu_local};
use core::arch::asm;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserCopyError {
    InvalidPointer,
    BufferTooSmall,
    RecoveryActive,
    Fault,
}

const USER_ADDRESS_LIMIT: u64 = 0x0000_8000_0000_0000;

fn validate_user_range(user_ptr: u64, len: usize) -> Result<u64, UserCopyError> {
    let end_ptr = user_ptr
        .checked_add(len as u64)
        .ok_or(UserCopyError::InvalidPointer)?;
    if user_ptr >= USER_ADDRESS_LIMIT || end_ptr > USER_ADDRESS_LIMIT {
        return Err(UserCopyError::InvalidPointer);
    }
    Ok(end_ptr)
}

/// Safely copies bytes from untrusted user space to kernel memory.
pub fn copy_from_user(dst: &mut [u8], src_user_ptr: u64, len: usize) -> Result<(), UserCopyError> {
    if len > dst.len() {
        return Err(UserCopyError::BufferTooSmall);
    }
    if len == 0 {
        return Ok(());
    }

    let end_ptr = validate_user_range(src_user_ptr, len)?;

    unsafe {
        let cpu_local = get_cpu_local();
        let mut fault_occurred: u64 = 0;

        let faulting_ip: u64;
        let recovery_ip: u64;
        asm!(
            "lea {faulting_ip}, [rip + 4f]",
            "lea {recovery_ip}, [rip + 5f]",
            faulting_ip = out(reg) faulting_ip,
            recovery_ip = out(reg) recovery_ip,
            options(nomem, nostack, preserves_flags)
        );

        let recovery = UserCopyRecovery {
            fault_resume_rip: recovery_ip,
            faulting_rip: faulting_ip,
            user_start: src_user_ptr,
            user_end: end_ptr,
        };
        cpu_local
            .install_recovery(&recovery)
            .map_err(|_| UserCopyError::RecoveryActive)?;

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

        cpu_local.clear_recovery(&recovery);

        if fault_occurred != 0 {
            Err(UserCopyError::Fault)
        } else {
            Ok(())
        }
    }
}

/// Safely copies bytes from kernel memory to untrusted user space.
pub fn copy_to_user(dst_user_ptr: u64, src: &[u8], len: usize) -> Result<(), UserCopyError> {
    if len > src.len() {
        return Err(UserCopyError::BufferTooSmall);
    }
    if len == 0 {
        return Ok(());
    }

    let end_ptr = validate_user_range(dst_user_ptr, len)?;

    unsafe {
        let cpu_local = get_cpu_local();
        let mut fault_occurred: u64 = 0;

        let faulting_ip: u64;
        let recovery_ip: u64;
        asm!(
            "lea {faulting_ip}, [rip + 4f]",
            "lea {recovery_ip}, [rip + 5f]",
            faulting_ip = out(reg) faulting_ip,
            recovery_ip = out(reg) recovery_ip,
            options(nomem, nostack, preserves_flags)
        );

        let recovery = UserCopyRecovery {
            fault_resume_rip: recovery_ip,
            faulting_rip: faulting_ip,
            user_start: dst_user_ptr,
            user_end: end_ptr,
        };
        cpu_local
            .install_recovery(&recovery)
            .map_err(|_| UserCopyError::RecoveryActive)?;

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

        cpu_local.clear_recovery(&recovery);

        if fault_occurred != 0 {
            Err(UserCopyError::Fault)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_length_cannot_exceed_kernel_slice() {
        let mut destination = [0_u8; 1];
        assert_eq!(
            copy_from_user(&mut destination, 0x1000, 2),
            Err(UserCopyError::BufferTooSmall)
        );
        assert_eq!(
            copy_to_user(0x1000, &[0_u8; 1], 2),
            Err(UserCopyError::BufferTooSmall)
        );
    }

    #[test]
    fn user_range_accepts_the_last_user_byte() {
        assert_eq!(
            validate_user_range(USER_ADDRESS_LIMIT - 1, 1),
            Ok(USER_ADDRESS_LIMIT)
        );
        assert_eq!(
            validate_user_range(USER_ADDRESS_LIMIT, 1),
            Err(UserCopyError::InvalidPointer)
        );
    }
}
