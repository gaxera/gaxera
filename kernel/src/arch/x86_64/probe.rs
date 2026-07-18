//! M2A Ring-3 Probe Runtime
//!
//! This module orchestrates the transition into the static M2A probe address space.
//! It is strictly for testing the isolated user transition bounds before a general
//! user task model exists.

use core::arch::asm;

#[allow(unused_imports)]
use crate::arch::x86_64::descriptors::{
    install_user_transition_stack, set_tss_rsp0, user_selectors,
};
use crate::arch::x86_64::paging::{KernelPageTables, PagingError, UserPageTables};
#[allow(unused_imports)]
use crate::arch::x86_64::user::{UserSelectors, UserTransitionError, UserTransitionFrame};
use crate::memory::physical::SegmentedBitmapFrameAllocator;
#[allow(unused_imports)]
use crate::println;

static mut KERNEL_CR3: u64 = 0;

pub struct M2AProbe {
    #[allow(dead_code)]
    page_tables: UserPageTables,
    selectors: UserSelectors,
}

impl M2AProbe {
    pub fn build(
        kernel_root: &KernelPageTables,
        allocator: &mut SegmentedBitmapFrameAllocator<'_>,
    ) -> Result<Self, PagingError> {
        // SAFETY: Interrupts are disabled during kernel bootstrap, and allocator is owned.
        let page_tables = unsafe { UserPageTables::build(kernel_root, allocator) }?;
        let selectors = user_selectors().expect("user selectors not initialized");
        Ok(Self {
            page_tables,
            selectors,
        })
    }

    /// Store kernel CR3 for test return.
    /// # Safety
    /// Single threaded early boot only.
    #[allow(dead_code)]
    unsafe fn stash_kernel_cr3() {
        let cr3: u64;
        unsafe { asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags)) };
        unsafe { KERNEL_CR3 = cr3 };
    }

    /// Restore kernel CR3 from the test return gate.
    /// # Safety
    /// Must only be called from the M2A test return gate.
    pub unsafe fn restore_kernel_cr3() {
        let cr3 = unsafe { KERNEL_CR3 };
        unsafe { asm!("mov cr3, {}", in(reg) cr3, options(nomem, nostack, preserves_flags)) };
    }

    /// Enter the M2A probe.
    pub fn execute(&self) -> ! {
        // SAFETY: the M2A bootstrap path is single-threaded with interrupts
        // disabled, and descriptor initialization has already completed.
        let top = unsafe { install_user_transition_stack() }.expect("Descriptors not initialized");
        self.execute_on_kernel_stack(top)
    }

    /// Enter the probe using a caller-owned kernel stack for all later ring-3
    /// to ring-0 transitions. M3 uses this path so the syscall frame belongs
    /// to the running thread rather than the M2A static test stack.
    pub fn execute_on_kernel_stack(&self, kernel_stack_top: u64) -> ! {
        let _ = kernel_stack_top;
        let frame = UserTransitionFrame::fixed_probe(self.selectors);

        #[cfg(feature = "test-user-invalid-frame")]
        {
            let mut invalid = frame;
            invalid.code_selector = 0x08;
            if let Err(UserTransitionError::InvalidCodeSelector) = invalid.validate(self.selectors)
            {
                println!("GAXERA: USER_INVALID_FRAME_REJECTED");
                unsafe { crate::arch::x86_64::qemu::exit_success() };
            }
            panic!("Invalid frame was not rejected");
        }

        #[cfg(not(feature = "test-user-invalid-frame"))]
        {
            if let Err(e) = frame.validate(self.selectors) {
                panic!("M2A valid frame rejected: {:?}", e);
            }

            // SAFETY: This is the single-threaded bootstrap path.
            unsafe {
                Self::stash_kernel_cr3();
                set_tss_rsp0(kernel_stack_top);
                crate::arch::x86_64::cpu::set_kernel_stack_top(kernel_stack_top);
                self.page_tables
                    .activate()
                    .expect("Failed to activate user CR3");

                // iretq sequence
                asm!(
                    "mov ds, {data:x}",
                    "mov es, {data:x}",
                    "push {data}",   // SS
                    "push {rsp}",    // RSP
                    "push {rflags}", // RFLAGS
                    "push {code}",   // CS
                    "push {rip}",    // RIP
                    "iretq",
                    data = in(reg) frame.stack_selector as u64,
                    rsp = in(reg) frame.stack_pointer,
                    rflags = in(reg) frame.rflags,
                    code = in(reg) frame.code_selector as u64,
                    rip = in(reg) frame.instruction_pointer,
                    options(noreturn)
                );
            }
        }
    }
}
