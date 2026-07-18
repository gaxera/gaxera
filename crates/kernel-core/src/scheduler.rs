use alloc::collections::VecDeque;

use crate::object::ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SchedulerError {
    CapacityTooLarge,
    AllocationFailed,
    QueueFull,
    InvalidState,
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
        if self.run_queue.len() >= self.capacity {
            return Err(SchedulerError::QueueFull);
        }
        thread
            .make_runnable()
            .map_err(|_| SchedulerError::InvalidState)?;
        self.run_queue.push_back(thread.id());
        Ok(())
    }

    pub fn dequeue_next(&mut self) -> Option<ObjectId> {
        // The caller is responsible for fetching the thread from the arena and
        // transitioning its state to Running using thread.make_running().
        self.run_queue.pop_front()
    }

    pub fn current_thread(&self) -> Option<ObjectId> {
        self.current_thread
    }

    pub fn set_current_thread(&mut self, thread: Option<ObjectId>) {
        self.current_thread = thread;
    }
}
