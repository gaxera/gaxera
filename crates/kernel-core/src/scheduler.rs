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
}

impl Scheduler {
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
}
