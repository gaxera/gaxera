use crate::arch::x86_64::trap_frame::TrapFrame;
use crate::arch::x86_64::{apic, cpu};
use kernel_core::object::ObjectId;
use kernel_core::scheduler::Scheduler;

#[unsafe(no_mangle)]
pub extern "C" fn timer_preempt_handler(_trap_frame: *mut TrapFrame) {
    timer_tick_and_maybe_preempt(true);
}

#[unsafe(no_mangle)]
pub extern "C" fn timer_kernel_tick() {
    timer_tick_and_maybe_preempt(false);
}

fn timer_tick_and_maybe_preempt(from_user: bool) {
    // SAFETY: Hardware invariant or verified by caller.
    let cpu_local = unsafe { cpu::get_cpu_local() };

    // 1. Advance MonotonicClock
    let _now = cpu_local.monotonic_clock.advance();

    // 2. Advance TimerQueue
    // SAFETY: Hardware invariant or verified by caller.
    let timer_queue_cell = unsafe { &mut *cpu_local.timer_queue.get() };
    if let Some(queue) = timer_queue_cell.as_mut() {
        queue.advance_to(_now, |_timer_id| {
            // Signal notifications (deferred until global object table is available)
        });
    }

    // 3. EOI APIC
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        apic::end_of_interrupt();
    }

    if !from_user {
        return;
    }

    // 4. Check Scheduler Quantum
    // SAFETY: Hardware invariant or verified by caller.
    let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
    if let Some(scheduler) = scheduler_cell.as_mut()
        && scheduler.tick()
    {
        // Quantum expired
        if let (Some(current_id), Some(next_id)) =
            (scheduler.current_thread(), scheduler.next_runnable())
        {
            let _ = reschedule(scheduler, current_id, next_id);
        }
    }
}

pub(crate) fn reschedule(
    scheduler: &mut Scheduler,
    current_id: ObjectId,
    next_id: ObjectId,
) -> Result<(), ()> {
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        crate::arch::x86_64::thread::THREADS
            .with_two_mut(current_id, next_id, |current, next| {
                if current.state() != kernel_core::thread::ThreadState::Running
                    || next.state() != kernel_core::thread::ThreadState::Runnable
                {
                    crate::println!(
                        "GAXERA ERROR: reschedule state mismatch: current={:?} next={:?}",
                        current.state(),
                        next.state()
                    );
                    return Err(());
                }

                if let Err(e) = scheduler.commit_yield(current_id, next_id) {
                    crate::println!("GAXERA ERROR: commit_yield failed: {:?}", e);
                    return Err(());
                }
                current.make_runnable().map_err(|_| ())?;
                next.make_running().map_err(|_| ())?;

                scheduler.reset_quantum();

                let current_context = &mut current.arch.context as *mut _;
                let next_context = &next.arch.context as *const _;
                let next_stack_top = next.arch.stack.top().as_u64();
                let next_cr3 = next.arch.cr3;

                // SAFETY: queue and thread state are committed as one BSP-only
                // transition; both contexts and the incoming stack are live.
                crate::arch::x86_64::context::switch_thread(
                    current_context,
                    next_context,
                    next_stack_top,
                    next_cr3,
                );
                Ok(())
            })
            .ok_or(())?
    }
}

pub(crate) fn switch_to_next(current_id: ObjectId, next_id: ObjectId) -> Result<(), ()> {
    // SAFETY: APIC is initialized, timer arming is safe at this level.
    unsafe {
        crate::arch::x86_64::thread::THREADS
            .with_two_mut(current_id, next_id, |current, next| {
                let _ = next.make_running();

                let current_context = &mut current.arch.context as *mut _;
                let next_context = &next.arch.context as *const _;
                let next_stack_top = next.arch.stack.top().as_u64();
                let next_cr3 = next.arch.cr3;

                crate::arch::x86_64::context::switch_thread(
                    current_context,
                    next_context,
                    next_stack_top,
                    next_cr3,
                );
                Ok(())
            })
            .ok_or(())?
    }
}
