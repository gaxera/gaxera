use alloc::collections::VecDeque;

use crate::object::ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    CapacityTooLarge,
    AllocationFailed,
    QueueFull,
    InvalidState,
    DuplicateEntry,
    NoCurrentThread,
    NoRunnableThread,
    QueueInvariant,
}

pub const PRIORITY_LEVELS: usize = 32;

pub struct Scheduler {
    queues: [VecDeque<ObjectId>; PRIORITY_LEVELS],
    active_bitmap: u32,
    current_thread: Option<ObjectId>,
    capacity: usize,
    count: usize,
    quantum_remaining: u32,
    quantum_size: u32,
}

impl Scheduler {
    pub const QUANTUM_TICKS: u32 = 10;

    pub fn try_new(capacity: usize) -> Result<Self, SchedulerError> {
        if capacity > u32::MAX as usize {
            return Err(SchedulerError::CapacityTooLarge);
        }

        // Initialize array of queues with pre-reserved capacity
        let mut queues: [VecDeque<ObjectId>; PRIORITY_LEVELS] = Default::default();
        for q in queues.iter_mut() {
            q.try_reserve_exact(capacity)
                .map_err(|_| SchedulerError::AllocationFailed)?;
        }

        Ok(Self {
            queues,
            active_bitmap: 0,
            current_thread: None,
            capacity,
            count: 0,
            quantum_remaining: Self::QUANTUM_TICKS,
            quantum_size: Self::QUANTUM_TICKS,
        })
    }

    /// Request direct thread switch from current thread to target receiver thread for fast-path IPC.
    pub fn try_direct_switch(
        &mut self,
        current_thread: ObjectId,
        target_receiver: ObjectId,
    ) -> Result<ObjectId, crate::ipc::FastPathRejectReason> {
        if self.current_thread != Some(current_thread) {
            return Err(crate::ipc::FastPathRejectReason::SchedulerDeclined);
        }
        // Direct switch: set current_thread to target_receiver while inheriting quantum
        self.current_thread = Some(target_receiver);
        Ok(target_receiver)
    }

    pub fn enqueue<T>(
        &mut self,
        thread: &mut crate::thread::Thread<T>,
    ) -> Result<(), SchedulerError> {
        let id = thread.id();
        if thread.state() == crate::thread::ThreadState::Runnable
            || self.contains(id)
            || self.current_thread == Some(id)
        {
            return Err(SchedulerError::DuplicateEntry);
        }
        if self.count >= self.capacity {
            return Err(SchedulerError::QueueFull);
        }
        thread
            .make_runnable()
            .map_err(|_| SchedulerError::InvalidState)?;

        let prio = (thread.effective_priority() as usize).min(PRIORITY_LEVELS - 1);
        self.queues[prio].push_back(id);
        self.active_bitmap |= 1u32 << prio;
        self.count += 1;
        Ok(())
    }

    pub fn dequeue_next(&mut self) -> Option<ObjectId> {
        if self.active_bitmap == 0 {
            return None;
        }
        let highest_prio = (31 - self.active_bitmap.leading_zeros()) as usize;
        let dequeued = self.queues[highest_prio].pop_front();
        if self.queues[highest_prio].is_empty() {
            self.active_bitmap &= !(1u32 << highest_prio);
        }
        if dequeued.is_some() {
            self.count = self.count.saturating_sub(1);
        }
        dequeued
    }

    /// Returns the next runnable ID without changing scheduler state.
    pub fn next_runnable(&self) -> Option<ObjectId> {
        if self.active_bitmap == 0 {
            return None;
        }
        let highest_prio = (31 - self.active_bitmap.leading_zeros()) as usize;
        self.queues[highest_prio].front().copied()
    }

    /// Commits the queue portion of a cooperative yield.
    pub fn commit_yield(
        &mut self,
        current: ObjectId,
        next: ObjectId,
    ) -> Result<(), SchedulerError> {
        self.commit_yield_with_priority(current, 0, next)
    }

    /// Commits the queue portion of a cooperative yield with explicit priority.
    pub fn commit_yield_with_priority(
        &mut self,
        current: ObjectId,
        current_prio: u8,
        next: ObjectId,
    ) -> Result<(), SchedulerError> {
        if self.current_thread != Some(current) {
            return Err(SchedulerError::NoCurrentThread);
        }
        let highest = match self.next_runnable() {
            Some(id) => id,
            None => return Err(SchedulerError::NoRunnableThread),
        };
        if current == next || highest != next {
            return Err(SchedulerError::NoRunnableThread);
        }

        let dequeued = self.dequeue_next();
        if dequeued != Some(next) {
            return Err(SchedulerError::QueueInvariant);
        }

        let prio = (current_prio as usize).min(PRIORITY_LEVELS - 1);
        self.queues[prio].push_back(current);
        self.active_bitmap |= 1u32 << prio;
        self.count += 1;

        self.current_thread = Some(next);
        Ok(())
    }

    pub fn current_thread(&self) -> Option<ObjectId> {
        self.current_thread
    }

    pub fn set_current_thread(&mut self, thread: Option<ObjectId>) {
        self.current_thread = thread;
    }

    /// Returns true if the given thread ID is already in any priority run queue.
    pub fn contains(&self, id: ObjectId) -> bool {
        if self.count == 0 {
            return false;
        }
        for q in &self.queues {
            if q.contains(&id) {
                return true;
            }
        }
        false
    }

    /// Blocks the current thread and unsets it from the scheduler.
    pub fn block_current<T>(
        &mut self,
        thread: &mut crate::thread::Thread<T>,
    ) -> Result<ObjectId, SchedulerError> {
        let id = thread.id();
        if self.current_thread != Some(id) {
            return Err(SchedulerError::NoCurrentThread);
        }
        thread
            .make_blocked()
            .map_err(|_| SchedulerError::InvalidState)?;
        self.current_thread = None;
        Ok(id)
    }

    /// Applies a wake effect to the target thread.
    pub fn apply_wake<T>(
        &mut self,
        thread: &mut crate::thread::Thread<T>,
    ) -> Result<(), SchedulerError> {
        if self.count >= self.capacity {
            return Err(SchedulerError::QueueFull);
        }

        match thread.state() {
            crate::thread::ThreadState::Blocked => {
                thread
                    .make_runnable()
                    .map_err(|_| SchedulerError::InvalidState)?;
                let prio = (thread.effective_priority() as usize).min(PRIORITY_LEVELS - 1);
                self.queues[prio].push_front(thread.id());
                self.active_bitmap |= 1u32 << prio;
                self.count += 1;
                Ok(())
            }
            crate::thread::ThreadState::Dead => Ok(()),
            crate::thread::ThreadState::Running | crate::thread::ThreadState::Runnable => Ok(()),
            _ => Err(SchedulerError::InvalidState),
        }
    }

    /// Decrements the current thread's quantum, returning true if it has expired.
    pub fn tick(&mut self) -> bool {
        self.quantum_remaining = self.quantum_remaining.saturating_sub(1);
        self.quantum_remaining == 0
    }

    /// Resets the current thread's quantum to the configured size.
    pub fn reset_quantum(&mut self) {
        self.quantum_remaining = self.quantum_size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thread::Thread;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct MockArch;

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn enqueue_rejects_duplicate() {
        let mut sched = Scheduler::try_new(4).unwrap();
        let mut t0 = Thread::new(test_id(0), None, MockArch);
        let mut t0_dup = Thread::new(test_id(0), None, MockArch);

        assert!(sched.enqueue(&mut t0).is_ok());
        // t0_dup has the same ObjectId — must be rejected
        let _ = t0_dup.make_runnable(); // force state for the duplicate check path
        let _ = t0_dup.make_running();
        assert_eq!(
            sched.enqueue(&mut t0_dup),
            Err(SchedulerError::DuplicateEntry)
        );
    }

    #[test]
    fn enqueue_rejects_full_queue() {
        let mut sched = Scheduler::try_new(1).unwrap();
        let mut t0 = Thread::new(test_id(0), None, MockArch);
        let mut t1 = Thread::new(test_id(1), None, MockArch);

        assert!(sched.enqueue(&mut t0).is_ok());
        let _ = t1.make_runnable();
        let _ = t1.make_running();
        assert_eq!(sched.enqueue(&mut t1), Err(SchedulerError::QueueFull));
    }

    #[test]
    fn scheduler_queue_logic() {
        let mut sched = Scheduler::try_new(2).unwrap();
        let mut t1 = Thread::new(test_id(1), None, MockArch);
        let mut t2 = Thread::new(test_id(2), None, MockArch);
        let mut t3 = Thread::new(test_id(3), None, MockArch);

        assert_eq!(sched.enqueue(&mut t1), Ok(()));
        assert_eq!(sched.enqueue(&mut t2), Ok(()));
        assert_eq!(sched.enqueue(&mut t3), Err(SchedulerError::QueueFull));

        assert_eq!(sched.dequeue_next(), Some(t1.id()));
        assert_eq!(sched.dequeue_next(), Some(t2.id()));
        assert_eq!(sched.dequeue_next(), None);
    }

    #[test]
    fn scheduler_block_current() {
        let mut sched = Scheduler::try_new(2).unwrap();
        let mut t1 = Thread::new(test_id(1), None, MockArch);

        // Setup current thread manually for the test
        assert_eq!(sched.enqueue(&mut t1), Ok(()));
        assert_eq!(sched.dequeue_next(), Some(t1.id()));
        assert_eq!(t1.make_running(), Ok(()));
        sched.set_current_thread(Some(t1.id()));

        // Block it
        assert_eq!(sched.block_current(&mut t1), Ok(t1.id()));
        assert_eq!(sched.current_thread(), None);
        assert_eq!(t1.state(), crate::thread::ThreadState::Blocked);

        // Attempting to block it again fails because it's no longer current
        assert_eq!(
            sched.block_current(&mut t1),
            Err(SchedulerError::NoCurrentThread)
        );
    }

    #[test]
    fn scheduler_apply_wake() {
        let mut sched = Scheduler::try_new(2).unwrap();
        let mut t1 = Thread::new(test_id(1), None, MockArch);

        // Force to Blocked state for testing
        let _ = t1.make_runnable();
        let _ = t1.make_running();
        assert_eq!(t1.make_blocked(), Ok(()));

        // Normal wake
        assert_eq!(sched.apply_wake(&mut t1), Ok(()));
        assert_eq!(t1.state(), crate::thread::ThreadState::Runnable);
        assert!(sched.contains(t1.id()));

        // Duplicate wake is ignored
        assert_eq!(sched.apply_wake(&mut t1), Ok(()));

        // Dead thread wake is ignored
        let mut t2 = Thread::new(test_id(2), None, MockArch);
        let _ = t2.make_runnable();
        let _ = t2.make_dying();
        let _ = t2.make_dead();
        assert_eq!(sched.apply_wake(&mut t2), Ok(()));
        assert!(!sched.contains(t2.id()));
    }

    #[test]
    fn dequeue_returns_fifo_order() {
        let mut sched = Scheduler::try_new(4).unwrap();
        let mut t0 = Thread::new(test_id(0), None, MockArch);
        let mut t1 = Thread::new(test_id(1), None, MockArch);

        assert!(sched.enqueue(&mut t0).is_ok());
        // t1 needs to be in a state that allows make_runnable
        let _ = t1.make_runnable();
        let _ = t1.make_running();
        assert!(sched.enqueue(&mut t1).is_ok());

        assert_eq!(sched.dequeue_next(), Some(test_id(0)));
        assert_eq!(sched.dequeue_next(), Some(test_id(1)));
        assert_eq!(sched.dequeue_next(), None);
    }

    #[test]
    fn contains_reports_presence() {
        let mut sched = Scheduler::try_new(4).unwrap();
        let mut t0 = Thread::new(test_id(0), None, MockArch);

        assert!(!sched.contains(test_id(0)));
        assert!(sched.enqueue(&mut t0).is_ok());
        assert!(sched.contains(test_id(0)));
    }

    #[test]
    fn current_thread_cannot_be_enqueued() {
        let mut sched = Scheduler::try_new(2).unwrap();
        let mut t0 = Thread::new(test_id(0), None, MockArch);
        t0.make_runnable().unwrap();
        t0.make_running().unwrap();
        sched.set_current_thread(Some(t0.id()));

        assert_eq!(sched.enqueue(&mut t0), Err(SchedulerError::DuplicateEntry));
    }

    #[test]
    fn committed_yield_rotates_fifo_without_allocation() {
        let mut sched = Scheduler::try_new(2).unwrap();
        let mut t0 = Thread::new(test_id(0), None, MockArch);
        let mut t1 = Thread::new(test_id(1), None, MockArch);
        t0.make_runnable().unwrap();
        t0.make_running().unwrap();
        sched.set_current_thread(Some(t0.id()));
        sched.enqueue(&mut t1).unwrap();

        assert_eq!(sched.commit_yield(t0.id(), t1.id()), Ok(()));
        assert_eq!(sched.current_thread(), Some(t1.id()));
        assert_eq!(sched.next_runnable(), Some(t0.id()));
    }

    #[test]
    fn scheduler_priority_ordering() {
        let mut sched = Scheduler::try_new(4).unwrap();
        let mut t_low = Thread::new(test_id(1), None, MockArch);
        t_low.set_base_priority(5);

        let mut t_med = Thread::new(test_id(2), None, MockArch);
        t_med.set_base_priority(12);

        let mut t_high = Thread::new(test_id(3), None, MockArch);
        t_high.set_base_priority(25);

        // Enqueue in arbitrary order: low, high, med
        assert_eq!(sched.enqueue(&mut t_low), Ok(()));
        assert_eq!(sched.enqueue(&mut t_high), Ok(()));
        assert_eq!(sched.enqueue(&mut t_med), Ok(()));

        // Dequeue should select strictly highest priority first: high (25) -> med (12) -> low (5)
        assert_eq!(sched.dequeue_next(), Some(t_high.id()));
        assert_eq!(sched.dequeue_next(), Some(t_med.id()));
        assert_eq!(sched.dequeue_next(), Some(t_low.id()));
        assert_eq!(sched.dequeue_next(), None);
    }

    #[test]
    fn scheduler_quantum_tracking() {
        let mut sched = Scheduler::try_new(2).unwrap();
        assert_eq!(sched.quantum_remaining, Scheduler::QUANTUM_TICKS);

        // Tick 9 times
        for _ in 0..9 {
            assert!(!sched.tick());
        }

        // 10th tick should expire
        assert!(sched.tick());

        // Saturation at 0
        assert!(sched.tick());

        // Reset restores it
        sched.reset_quantum();
        assert_eq!(sched.quantum_remaining, Scheduler::QUANTUM_TICKS);
        assert!(!sched.tick());
    }
}
