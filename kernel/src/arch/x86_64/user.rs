//! M2A's internal ring-3 entry contract.
//!
//! This module validates only the fixed probe frame. It does not define a
//! syscall ABI, user-pointer access, ELF loading, or a general task context.

use crate::memory::mapping::{USER_ADDRESS_MAX, USER_PROBE_CODE, USER_STACK_PAGE, USER_STACK_TOP};

pub const USER_INITIAL_RFLAGS: u64 = 1 << 1;
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
