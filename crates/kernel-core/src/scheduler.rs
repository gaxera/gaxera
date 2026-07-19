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

pub struct Scheduler {
    run_queue: VecDeque<ObjectId>,
    current_thread: Option<ObjectId>,
    capacity: usize,
    quantum_remaining: u32,
    quantum_size: u32,
}

impl Scheduler {
    pub const QUANTUM_TICKS: u32 = 10;
    pub fn try_new(capacity: usize) -> Result<Self, SchedulerError> {
        if capacity > u32::MAX as usize {
            return Err(SchedulerError::CapacityTooLarge);
        }
        let mut run_queue = VecDeque::new();
        run_queue
            .try_reserve_exact(capacity)
            .map_err(|_| SchedulerError::AllocationFailed)?;
        Ok(Self {
            run_queue,
            current_thread: None,
            capacity,
            quantum_remaining: Self::QUANTUM_TICKS,
            quantum_size: Self::QUANTUM_TICKS,
        })
    }

    pub fn enqueue<T>(
        &mut self,
        thread: &mut crate::thread::Thread<T>,
    ) -> Result<(), SchedulerError> {
        let id = thread.id();
        if self.run_queue.contains(&id) {
            return Err(SchedulerError::DuplicateEntry);
        }
        if self.current_thread == Some(id) {
            return Err(SchedulerError::DuplicateEntry);
        }
        if self.run_queue.len() >= self.capacity {
            return Err(SchedulerError::QueueFull);
        }
        thread
            .make_runnable()
            .map_err(|_| SchedulerError::InvalidState)?;
        self.run_queue.push_back(id);
        Ok(())
    }

    pub fn dequeue_next(&mut self) -> Option<ObjectId> {
        // The caller is responsible for fetching the thread from the arena and
        // transitioning its state to Running using thread.make_running().
        self.run_queue.pop_front()
    }

    /// Returns the next runnable ID without changing scheduler state.
    pub fn next_runnable(&self) -> Option<ObjectId> {
        self.run_queue.front().copied()
    }

    /// Commits the queue portion of a cooperative yield.
    ///
    /// Thread-state validation and transitions remain owned by the caller so
    /// the scheduler does not acquire ownership of thread storage. This method
    /// performs no allocation: it removes one entry and reuses that capacity
    /// to place the former current thread at the FIFO tail.
    pub fn commit_yield(
        &mut self,
        current: ObjectId,
        next: ObjectId,
    ) -> Result<(), SchedulerError> {
        if self.current_thread != Some(current) {
            return Err(SchedulerError::NoCurrentThread);
        }
        if current == next || self.run_queue.front().copied() != Some(next) {
            return Err(SchedulerError::NoRunnableThread);
        }
        if self.run_queue.iter().skip(1).any(|id| *id == current) {
            return Err(SchedulerError::QueueInvariant);
        }

        let dequeued = self.run_queue.pop_front();
        if dequeued != Some(next) {
            return Err(SchedulerError::QueueInvariant);
        }
        self.run_queue.push_back(current);
        self.current_thread = Some(next);
        Ok(())
    }

    pub fn current_thread(&self) -> Option<ObjectId> {
        self.current_thread
    }

    pub fn set_current_thread(&mut self, thread: Option<ObjectId>) {
        self.current_thread = thread;
    }

    /// Returns true if the given thread ID is already in the run queue.
    pub fn contains(&self, id: ObjectId) -> bool {
        self.run_queue.contains(&id)
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
    /// Ignores duplicate wakes on runnable/running threads and dead threads.
    pub fn apply_wake<T>(
        &mut self,
        thread: &mut crate::thread::Thread<T>,
    ) -> Result<(), SchedulerError> {
        if self.run_queue.len() >= self.capacity {
            return Err(SchedulerError::QueueFull);
        }

        match thread.state() {
            crate::thread::ThreadState::Blocked => {
                thread
                    .make_runnable()
                    .map_err(|_| SchedulerError::InvalidState)?;
                self.run_queue.push_back(thread.id());
                Ok(())
            }
            crate::thread::ThreadState::Dead => {
                Ok(()) // dead thread ignores
            }
            crate::thread::ThreadState::Running | crate::thread::ThreadState::Runnable => {
                Ok(()) // duplicate wake ignores
            }
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
        assert_eq!(sched.contains(t1.id()), true);

        // Duplicate wake is ignored
        assert_eq!(sched.apply_wake(&mut t1), Ok(()));

        // Dead thread wake is ignored
        let mut t2 = Thread::new(test_id(2), None, MockArch);
        let _ = t2.make_runnable();
        let _ = t2.make_dying();
        let _ = t2.make_dead();
        assert_eq!(sched.apply_wake(&mut t2), Ok(()));
        assert_eq!(sched.contains(t2.id()), false);
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
    fn scheduler_quantum_tracking() {
        let mut sched = Scheduler::try_new(2).unwrap();
        assert_eq!(sched.quantum_remaining, Scheduler::QUANTUM_TICKS);

        // Tick 9 times
        for _ in 0..9 {
            assert_eq!(sched.tick(), false);
        }

        // 10th tick should expire
        assert_eq!(sched.tick(), true);

        // Saturation at 0
        assert_eq!(sched.tick(), true);

        // Reset restores it
        sched.reset_quantum();
        assert_eq!(sched.quantum_remaining, Scheduler::QUANTUM_TICKS);
        assert_eq!(sched.tick(), false);
    }
}
