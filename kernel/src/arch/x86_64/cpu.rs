use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::VirtAddr;
use x86_64::registers::model_specific::KernelGsBase;

/// Minimal architecture-private recovery context for user-copy operations.
///
/// This encapsulates the active state for a recoverable user-access block.
/// For M2B, this holds just the instruction pointer to jump to upon fault,
/// but it is explicitly designed as a distinct abstraction to allow adding
/// fault metadata (e.g., faulting address, error flags) later without
/// rewriting the architectural contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserCopyRecovery {
    pub fault_resume_rip: u64,
}

/// CPU-local data structure accessed via `gs` register after `swapgs` (or normally in ring 0).
#[repr(C)]
pub struct CpuLocal {
    /// Pointer to the top of the current thread's kernel stack.
    /// Loaded into RSP during `syscall` entry.
    pub kernel_stack_top: u64,

    /// Stashed user RSP during syscall entry (since syscall does not push RSP).
    pub scratch_user_rsp: u64,

    /// Recovery context for active recoverable user-access operations.
    /// Represented as an atomic u64 (pointer or raw rip) for fast access.
    /// We use u64 instead of AtomicPtr to avoid Option boxing, 0 means no recovery active.
    pub user_copy_recovery_rip: AtomicU64,
}

impl CpuLocal {
    pub const fn new() -> Self {
        Self {
            kernel_stack_top: 0,
            scratch_user_rsp: 0,
            user_copy_recovery_rip: AtomicU64::new(0),
        }
    }

    /// Sets the active recovery context.
    pub fn set_recovery(&self, recovery: UserCopyRecovery) {
        self.user_copy_recovery_rip
            .store(recovery.fault_resume_rip, Ordering::Release);
    }

    /// Clears the active recovery context.
    pub fn clear_recovery(&self) {
        self.user_copy_recovery_rip.store(0, Ordering::Release);
    }

    /// Takes the active recovery context, clearing it if one was set.
    pub fn take_recovery(&self) -> Option<UserCopyRecovery> {
        let rip = self.user_copy_recovery_rip.swap(0, Ordering::Acquire);
        if rip != 0 {
            Some(UserCopyRecovery {
                fault_resume_rip: rip,
            })
        } else {
            None
        }
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
    unsafe {
        let gs_base = x86_64::registers::model_specific::GsBase::read().as_u64();
        &*(gs_base as *const CpuLocal)
    }
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
    }
}
