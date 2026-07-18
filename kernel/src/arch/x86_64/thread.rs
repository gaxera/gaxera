use alloc::vec::Vec;
use core::cell::UnsafeCell;

use kernel_core::object::ObjectId;
use x86_64::structures::paging::PhysFrame;

use crate::arch::x86_64::context::Context;
use crate::arch::x86_64::stack::KernelStack;
use crate::arch::x86_64::user::UserTransitionFrame;
use core::arch::global_asm;

global_asm!(
    r#"
.global thread_start_trampoline
.type thread_start_trampoline, @function
thread_start_trampoline:
    // When context_switch returns here, the next values on the stack
    // form a hardware interrupt return frame (SS, RSP, RFLAGS, CS, RIP).
    // We execute iretq to drop into ring 3.
    iretq
"#
);

unsafe extern "C" {
    fn thread_start_trampoline();
}

pub struct ArchThread {
    pub stack: KernelStack,
    pub context: Context,
    pub cr3: Option<PhysFrame>,
}

pub type Thread = kernel_core::thread::Thread<ArchThread>;

// SAFETY: M3 is strictly single BSP. We do not have SMP yet.
// We will transition this to a thread-safe slab allocator in M5 (SMP).
pub struct ThreadTable {
    threads: UnsafeCell<Vec<Option<Thread>>>,
}

unsafe impl Sync for ThreadTable {}

impl ThreadTable {
    pub const fn new() -> Self {
        Self {
            threads: UnsafeCell::new(Vec::new()),
        }
    }
}

impl Default for ThreadTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ThreadTable {
    /// # Safety
    /// Must only be called on the BSP.
    pub unsafe fn insert(&self, thread: Thread) {
        let threads = unsafe { &mut *self.threads.get() };
        let index = thread.id().index() as usize;
        if threads.len() <= index {
            threads.resize_with(index + 1, || None);
        }
        threads[index] = Some(thread);
    }

    /// # Safety
    /// Must only be called on the BSP.
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn get_mut(&self, id: ObjectId) -> Option<&mut Thread> {
        let threads = unsafe { &mut *self.threads.get() };
        let index = id.index() as usize;
        threads.get_mut(index).and_then(|opt| opt.as_mut())
    }

    /// # Safety
    /// Must only be called on the BSP.
    pub unsafe fn remove(&self, id: ObjectId) -> Option<Thread> {
        let threads = unsafe { &mut *self.threads.get() };
        let index = id.index() as usize;
        if index < threads.len() {
            threads[index].take()
        } else {
            None
        }
    }
}

pub static THREADS: ThreadTable = ThreadTable::new();

pub fn spawn_user_thread(
    stack: KernelStack,
    cr3: Option<PhysFrame>,
    frame: UserTransitionFrame,
) -> ArchThread {
    let mut rsp = stack.top().as_u64();

    unsafe {
        // Push UserTransitionFrame (for iretq)
        rsp -= 8;
        *(rsp as *mut u64) = frame.stack_selector as u64;
        rsp -= 8;
        *(rsp as *mut u64) = frame.stack_pointer;
        rsp -= 8;
        *(rsp as *mut u64) = frame.rflags;
        rsp -= 8;
        *(rsp as *mut u64) = frame.code_selector as u64;
        rsp -= 8;
        *(rsp as *mut u64) = frame.instruction_pointer;

        // Push the trampoline address (return address for context_switch)
        rsp -= 8;
        *(rsp as *mut u64) = thread_start_trampoline as *const () as u64;

        // Push callee-saved registers (rbp, rbx, r12, r13, r14, r15) = 48 bytes
        rsp -= 48;
        core::ptr::write_bytes(rsp as *mut u8, 0, 48);
    }

    ArchThread {
        stack,
        context: Context { rsp },
        cr3,
    }
}
