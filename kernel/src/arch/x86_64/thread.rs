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

/// A slot in the thread table, carrying the generation at insertion time.
///
/// Lookups compare the requested `ObjectId.generation()` against the stored
/// generation to prevent stale IDs from aliasing a reused slot.
struct ThreadSlot {
    generation: u32,
    thread: Option<Thread>,
}

// SAFETY: M3 is strictly single BSP. We do not have SMP yet.
// We will transition this to a thread-safe slab allocator in M5 (SMP).
pub struct ThreadTable {
    slots: UnsafeCell<Vec<ThreadSlot>>,
}

unsafe impl Sync for ThreadTable {}

impl ThreadTable {
    pub const fn new() -> Self {
        Self {
            slots: UnsafeCell::new(Vec::new()),
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
        let slots = unsafe { &mut *self.slots.get() };
        let index = thread.id().index() as usize;
        let generation = thread.id().generation();
        if slots.len() <= index {
            slots.resize_with(index + 1, || ThreadSlot {
                generation: 0,
                thread: None,
            });
        }
        slots[index] = ThreadSlot {
            generation,
            thread: Some(thread),
        };
    }

    /// # Safety
    /// Must only be called on the BSP.
    pub unsafe fn with_two_mut<R>(
        &self,
        first: ObjectId,
        second: ObjectId,
        operation: impl FnOnce(&mut Thread, &mut Thread) -> R,
    ) -> Option<R> {
        if first == second {
            return None;
        }

        let slots = unsafe { &mut *self.slots.get() };
        let first_index = first.index() as usize;
        let second_index = second.index() as usize;

        let (first_slot, second_slot) = if first_index < second_index {
            let (left, right) = slots.split_at_mut(second_index);
            (left.get_mut(first_index)?, right.first_mut()?)
        } else {
            let (left, right) = slots.split_at_mut(first_index);
            (right.first_mut()?, left.get_mut(second_index)?)
        };

        if first_slot.generation != first.generation()
            || second_slot.generation != second.generation()
        {
            return None;
        }

        Some(operation(
            first_slot.thread.as_mut()?,
            second_slot.thread.as_mut()?,
        ))
    }

    /// # Safety
    /// Must only be called on the BSP.
    pub unsafe fn remove(&self, id: ObjectId) -> Option<Thread> {
        let slots = unsafe { &mut *self.slots.get() };
        let index = id.index() as usize;
        if index < slots.len() && slots[index].generation == id.generation() {
            slots[index].thread.take()
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
