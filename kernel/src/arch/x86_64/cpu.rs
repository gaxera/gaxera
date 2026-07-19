use core::sync::atomic::{AtomicPtr, Ordering};
use x86_64::VirtAddr;
use x86_64::registers::model_specific::KernelGsBase;

/// Minimal architecture-private recovery context for user-copy operations.
///
/// This encapsulates the active state for a recoverable user-access block.
/// The record describes one exact faultable `rep movsb` instruction and the
/// user range it is permitted to access. It lives on the current kernel stack
/// for the duration of the copy; the CPU-local field stores only its pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserCopyRecovery {
    pub fault_resume_rip: u64,
    pub faulting_rip: u64,
    pub user_start: u64,
    pub user_end: u64,
}

/// CPU-local data structure accessed via `gs` register after `swapgs` (or normally in ring 0).
#[repr(C)]
pub struct CpuLocal {
    /// Pointer to the top of the current thread's kernel stack.
    /// Loaded into RSP during `syscall` entry.
    pub kernel_stack_top: u64,

    /// Stashed user RSP during syscall entry (since syscall does not push RSP).
    pub scratch_user_rsp: u64,

    /// Pointer to the one active recoverable user-access record, or null.
    pub user_copy_recovery: AtomicPtr<UserCopyRecovery>,

    /// The processor-local thread scheduler.
    pub scheduler: core::cell::UnsafeCell<Option<kernel_core::scheduler::Scheduler>>,

    /// The processor-local monotonic clock.
    pub monotonic_clock: kernel_core::time::MonotonicClock,

    /// The processor-local timer queue.
    pub timer_queue: core::cell::UnsafeCell<Option<kernel_core::timer::TimerQueue>>,
}

impl CpuLocal {
    pub const fn new() -> Self {
        Self {
            kernel_stack_top: 0,
            scratch_user_rsp: 0,
            user_copy_recovery: AtomicPtr::new(core::ptr::null_mut()),
            scheduler: core::cell::UnsafeCell::new(None),
            monotonic_clock: kernel_core::time::MonotonicClock::new(),
            timer_queue: core::cell::UnsafeCell::new(None),
        }
    }

    /// Installs a non-nestable recovery record for the current CPU.
    ///
    /// The caller must keep `recovery` live until it clears the record or a
    /// matching page fault has resumed execution at its landing pad.
    #[allow(clippy::result_unit_err)]
    pub fn install_recovery(&self, recovery: &UserCopyRecovery) -> Result<(), ()> {
        self.user_copy_recovery
            .compare_exchange(
                core::ptr::null_mut(),
                recovery as *const _ as *mut _,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .map(|_| ())
            .map_err(|_| ())
    }

    /// Clears this exact recovery record without disturbing a later record.
    pub fn clear_recovery(&self, recovery: &UserCopyRecovery) {
        let _ = self.user_copy_recovery.compare_exchange(
            recovery as *const _ as *mut _,
            core::ptr::null_mut(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    /// Takes the active recovery context, clearing it if one was set.
    pub fn take_recovery(&self) -> Option<UserCopyRecovery> {
        let recovery = self
            .user_copy_recovery
            .swap(core::ptr::null_mut(), Ordering::AcqRel);
        if recovery.is_null() {
            return None;
        }

        // SAFETY: `install_recovery` accepts only a stack record that remains
        // live until this CPU clears it or resumes through its recovery label.
        Some(unsafe { *recovery })
    }
}

impl Default for CpuLocal {
    fn default() -> Self {
        Self::new()
    }
}

static mut BSP_CPU_LOCAL: CpuLocal = CpuLocal::new();

/// Initializes the Kernel GS Base for the bootstrap processor.
/// # Safety
/// Single-threaded early boot only. Must be called before `swapgs` or user entry.
pub unsafe fn init_bsp_cpu_local() {
    let cpu_local_ptr = &raw mut BSP_CPU_LOCAL as u64;
    KernelGsBase::write(VirtAddr::new(cpu_local_ptr));
    // Also initialize regular GS base to the same, so it's valid immediately.
    x86_64::registers::model_specific::GsBase::write(VirtAddr::new(cpu_local_ptr));
}

/// Returns a reference to the active `CpuLocal` for the current processor.
///
/// # Safety
/// Must only be called when `GSBase` contains the `CpuLocal` pointer (i.e. in ring 0).
pub unsafe fn get_cpu_local() -> &'static CpuLocal {
    let ptr = x86_64::registers::model_specific::GsBase::read().as_u64() as *const CpuLocal;
    &*ptr
}

/// Returns a mutable reference to the active `CpuLocal`.
pub unsafe fn get_cpu_local_mut() -> &'static mut CpuLocal {
    let ptr = x86_64::registers::model_specific::GsBase::read().as_u64() as *mut CpuLocal;
    &mut *ptr
}

/// Sets the top of the kernel stack for the active processor.
///
/// # Safety
/// Must only be called in ring 0.
pub unsafe fn set_kernel_stack_top(top: u64) {
    unsafe {
        let gs_base = x86_64::registers::model_specific::GsBase::read().as_u64();
        let cpu_local = &mut *(gs_base as *mut CpuLocal);
        cpu_local.kernel_stack_top = top;
        crate::arch::x86_64::descriptors::set_tss_rsp0(top);
    }
}
