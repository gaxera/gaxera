//! M2A's internal ring-3 entry contract.
//!
//! This module validates only the fixed probe frame. It does not define a
//! syscall ABI, user-pointer access, ELF loading, or a general task context.

use crate::memory::mapping::{USER_ADDRESS_MAX, USER_PROBE_CODE, USER_STACK_PAGE, USER_STACK_TOP};

pub const USER_INITIAL_RFLAGS: u64 = (1 << 1) | (1 << 9); // Reserved bit 1 + IF
#[allow(dead_code)]
pub(crate) const USER_RETURN_VECTOR: u8 = 0x81;

// Default probe: int3, int 0x81 (return gate), ud2
#[cfg(not(any(
    feature = "test-cooperative-yield",
    feature = "test-context-preservation",
    feature = "test-syscall-round-trip",
    feature = "test-user-privilege",
    feature = "test-preemption"
)))]
pub(crate) const PROBE_BYTES: [u8; 5] = [0xcc, 0xcd, USER_RETURN_VECTOR, 0x0f, 0x0b];

// Syscall round-trip probe: mov eax, 0 (NoOp) / syscall / int 0x81 / ud2
#[cfg(feature = "test-syscall-round-trip")]
pub(crate) const PROBE_BYTES: [u8; 12] = [
    0xb8,
    0x00,
    0x00,
    0x00,
    0x00, // mov eax, 0 (NoOp)
    0x0f,
    0x05, // syscall
    0x90, // nop
    0xcd,
    USER_RETURN_VECTOR, // int 0x81
    0x0f,
    0x0b, // ud2
];

// Privilege denial probe: cli (privileged, triggers #GP from CPL 3)
#[cfg(feature = "test-user-privilege")]
pub(crate) const PROBE_BYTES: [u8; 5] = [
    0xfa, // cli — privileged instruction, triggers #GP at CPL 3
    0xcd,
    USER_RETURN_VECTOR, // int 0x81 (should never reach here)
    0x0f,
    0x0b, // ud2
];

// Cooperative yield probe: mov eax, 1 (yield) / syscall / nop / int 0x81 / ud2
#[cfg(any(
    feature = "test-cooperative-yield",
    feature = "test-context-preservation"
))]
pub(crate) const PROBE_BYTES: [u8; 12] = [
    0xb8,
    0x01,
    0x00,
    0x00,
    0x00, // mov eax, 1 (yield)
    0x0f,
    0x05, // syscall
    0x90, // nop (replaces int3 to avoid #GP)
    0xcd,
    USER_RETURN_VECTOR, // int 0x81
    0x0f,
    0x0b, // ud2
];
// Preemption probe: spin forever (offset 0), or exit success (offset 2)
#[cfg(feature = "test-preemption")]
pub(crate) const PROBE_BYTES: [u8; 11] = [
    0xeb, 0xfe, // jmp -2 (offset 0)
    0xb8, 0x02, 0x00, 0x00, 0x00, // mov eax, 2 (offset 2)
    0x0f, 0x05, // syscall
    0x0f, 0x0b, // ud2
];

const RFLAGS_FIXED_ONE: u64 = 1 << 1;
const RFLAGS_FORBIDDEN: u64 = (3 << 12) | (1 << 14) | (1 << 17) | (1 << 18);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserSelectors {
    pub code: u16,
    pub data: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserTransitionFrame {
    pub instruction_pointer: u64,
    pub stack_pointer: u64,
    pub rflags: u64,
    pub code_selector: u16,
    pub stack_selector: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserTransitionError {
    InstructionPointerOutsideProbe,
    StackPointerOutsideProbe,
    StackPointerMisaligned,
    InvalidFlags,
    InvalidCodeSelector,
    InvalidStackSelector,
}

impl UserTransitionFrame {
    pub const fn fixed_probe(selectors: UserSelectors) -> Self {
        Self {
            instruction_pointer: USER_PROBE_CODE,
            stack_pointer: USER_STACK_TOP,
            rflags: USER_INITIAL_RFLAGS,
            code_selector: selectors.code,
            stack_selector: selectors.data,
        }
    }

    pub fn validate(self, selectors: UserSelectors) -> Result<(), UserTransitionError> {
        if self.instruction_pointer != USER_PROBE_CODE
            || !is_canonical_lower_half(self.instruction_pointer)
        {
            return Err(UserTransitionError::InstructionPointerOutsideProbe);
        }
        if self.stack_pointer != USER_STACK_TOP
            || self.stack_pointer <= USER_STACK_PAGE
            || self.stack_pointer > USER_ADDRESS_MAX
            || !is_canonical_lower_half(self.stack_pointer)
        {
            return Err(UserTransitionError::StackPointerOutsideProbe);
        }
        if !self.stack_pointer.is_multiple_of(16) {
            return Err(UserTransitionError::StackPointerMisaligned);
        }
        if self.rflags != USER_INITIAL_RFLAGS
            || self.rflags & RFLAGS_FIXED_ONE == 0
            || self.rflags & RFLAGS_FORBIDDEN != 0
        {
            return Err(UserTransitionError::InvalidFlags);
        }
        if self.code_selector != selectors.code || self.code_selector & 0b11 != 0b11 {
            return Err(UserTransitionError::InvalidCodeSelector);
        }
        if self.stack_selector != selectors.data || self.stack_selector & 0b11 != 0b11 {
            return Err(UserTransitionError::InvalidStackSelector);
        }
        Ok(())
    }
}

const fn is_canonical_lower_half(address: u64) -> bool {
    address <= USER_ADDRESS_MAX
}

/// Hands off control to user space via `sysretq`.
///
/// # Safety
/// This function executes an unprotected hardware privilege transition. `entry_point`
/// and `stack_pointer` must be valid canonical user addresses properly mapped in the
/// current page table.
pub unsafe fn enter_user_mode(entry_point: u64, stack_pointer: u64, arg0: u64) -> ! {
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        core::arch::asm!(
            "swapgs",         // swap to user GS base
            "mov rsp, {stack}", // set user stack pointer
            // Clear remaining caller-saved and callee-saved registers to prevent kernel information leak
            "xor rax, rax",
            "xor rbx, rbx",
            "xor rdx, rdx",
            "xor rsi, rsi",
            "xor r8, r8",
            "xor r9, r9",
            "xor r10, r10",
            "xor r12, r12",
            "xor r13, r13",
            "xor r14, r14",
            "xor r15, r15",
            "xor rbp, rbp",
            "sysretq",
            stack = in(reg) stack_pointer,
            in("rcx") entry_point,
            in("r11") USER_INITIAL_RFLAGS,
            in("rdi") arg0,
            options(noreturn)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SELECTORS: UserSelectors = UserSelectors {
        code: 0x1b,
        data: 0x23,
    };

    #[test]
    fn fixed_probe_frame_is_valid() {
        assert_eq!(
            UserTransitionFrame::fixed_probe(SELECTORS).validate(SELECTORS),
            Ok(())
        );
    }

    #[test]
    fn validator_rejects_hostile_return_state() {
        let valid = UserTransitionFrame::fixed_probe(SELECTORS);
        assert_eq!(
            UserTransitionFrame {
                instruction_pointer: USER_PROBE_CODE + 1,
                ..valid
            }
            .validate(SELECTORS),
            Err(UserTransitionError::InstructionPointerOutsideProbe)
        );
        assert_eq!(
            UserTransitionFrame {
                stack_pointer: USER_STACK_TOP - 1,
                ..valid
            }
            .validate(SELECTORS),
            Err(UserTransitionError::StackPointerOutsideProbe)
        );
        assert_eq!(
            UserTransitionFrame {
                rflags: USER_INITIAL_RFLAGS | (3 << 12),
                ..valid
            }
            .validate(SELECTORS),
            Err(UserTransitionError::InvalidFlags)
        );
        assert_eq!(
            UserTransitionFrame {
                code_selector: 0x08,
                ..valid
            }
            .validate(SELECTORS),
            Err(UserTransitionError::InvalidCodeSelector)
        );
    }
}
