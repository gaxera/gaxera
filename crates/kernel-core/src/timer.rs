use alloc::vec::Vec;

use crate::object::ObjectId;
use crate::time::Deadline;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerError {
    Closed,
    QueueFull,
    NotArmed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerState {
    Idle,
    Armed {
        deadline: Deadline,
        notification_id: ObjectId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerEffect {
    Arm {
        timer_id: ObjectId,
        deadline: Deadline,
    },
    Replace {
        timer_id: ObjectId,
        new_deadline: Deadline,
    },
    Cancel {
        timer_id: ObjectId,
    },
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerCloseEffects {
    pub cancel_queue: bool,
}

pub struct TimerObject {
    id: ObjectId,
    state: TimerState,
    closed: bool,
}

impl TimerObject {
    pub fn new(id: ObjectId) -> Self {
        Self {
            id,
            state: TimerState::Idle,
            closed: false,
        }
    }

    pub fn arm(
        &mut self,
        deadline: Deadline,
        notification_id: ObjectId,
    ) -> Result<TimerEffect, TimerError> {
        if self.closed {
            return Err(TimerError::Closed);
        }
        let effect = match self.state {
            TimerState::Idle => TimerEffect::Arm {
                timer_id: self.id,
                deadline,
            },
            TimerState::Armed { .. } => TimerEffect::Replace {
                timer_id: self.id,
                new_deadline: deadline,
            },
        };
        self.state = TimerState::Armed {
            deadline,
            notification_id,
        };
        Ok(effect)
    }

    pub fn cancel(&mut self) -> Result<TimerEffect, TimerError> {
        if self.closed {
            return Err(TimerError::Closed);
        }
        match self.state {
            TimerState::Idle => Err(TimerError::NotArmed),
            TimerState::Armed { .. } => {
                self.state = TimerState::Idle;
                Ok(TimerEffect::Cancel { timer_id: self.id })
            }
        }
    }

    pub fn fire(&mut self) -> Result<ObjectId, TimerError> {
        if self.closed {
            return Err(TimerError::Closed);
        }
        match self.state {
            TimerState::Idle => Err(TimerError::NotArmed),
            TimerState::Armed {
                notification_id, ..
            } => {
                self.state = TimerState::Idle;
                Ok(notification_id)
            }
        }
    }

    pub fn close(&mut self) -> TimerCloseEffects {
        self.closed = true;
        match self.state {
            TimerState::Idle => TimerCloseEffects {
                cancel_queue: false,
            },
            TimerState::Armed { .. } => {
                self.state = TimerState::Idle;
                TimerCloseEffects { cancel_queue: true }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimerEntry {
    pub deadline: Deadline,
    pub timer_id: ObjectId,
}

pub struct TimerQueue {
    entries: Vec<TimerEntry>,
    capacity: usize,
}

impl TimerQueue {
    pub fn try_new(capacity: usize) -> Result<Self, TimerError> {
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(capacity)
            .map_err(|_| TimerError::QueueFull)?;
        Ok(Self { entries, capacity })
    }

    pub fn insert(&mut self, entry: TimerEntry) -> Result<(), TimerError> {
        if self.entries.len() >= self.capacity {
            return Err(TimerError::QueueFull);
        }
        let pos = self
            .entries
            .binary_search_by(|e| e.deadline.cmp(&entry.deadline))
            .unwrap_or_else(|e| e);
        self.entries.insert(pos, entry);
        Ok(())
    }

    pub fn remove(&mut self, timer_id: ObjectId) {
        if let Some(pos) = self.entries.iter().position(|e| e.timer_id == timer_id) {
            self.entries.remove(pos);
        }
    }

    pub fn advance_to<F>(&mut self, now: crate::time::Ticks, mut callback: F)
    where
        F: FnMut(ObjectId),
    {
        let expired_count = self
            .entries
            .iter()
            .take_while(|e| e.deadline.is_expired(now))
            .count();
        for entry in self.entries.drain(0..expired_count) {
            callback(entry.timer_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::Ticks;
    use alloc::vec;

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn timer_object_arm_fire_cancel() {
        let mut timer = TimerObject::new(test_id(1));
        let deadline = Deadline::new(Ticks(100));
        let notif_id = test_id(2);

        assert_eq!(timer.cancel(), Err(TimerError::NotArmed));
        assert_eq!(timer.fire(), Err(TimerError::NotArmed));

        let effect = timer.arm(deadline, notif_id).unwrap();
        assert_eq!(
            effect,
            TimerEffect::Arm {
                timer_id: test_id(1),
                deadline
            }
        );

        let fire_res = timer.fire().unwrap();
        assert_eq!(fire_res, notif_id);
        assert_eq!(timer.fire(), Err(TimerError::NotArmed));

        timer.arm(deadline, notif_id).unwrap();
        let cancel_res = timer.cancel().unwrap();
        assert_eq!(
            cancel_res,
            TimerEffect::Cancel {
                timer_id: test_id(1)
            }
        );
        assert_eq!(timer.fire(), Err(TimerError::NotArmed));
    }

    #[test]
    fn timer_object_rearm() {
        let mut timer = TimerObject::new(test_id(1));
        let d1 = Deadline::new(Ticks(100));
        let d2 = Deadline::new(Ticks(200));
        let notif_id = test_id(2);

        timer.arm(d1, notif_id).unwrap();
        let effect = timer.arm(d2, notif_id).unwrap();
        assert_eq!(
            effect,
            TimerEffect::Replace {
                timer_id: test_id(1),
                new_deadline: d2
            }
        );
    }

    #[test]
    fn timer_object_close() {
        let mut t1 = TimerObject::new(test_id(1));
        let effects1 = t1.close();
        assert!(!effects1.cancel_queue);
        assert_eq!(
            t1.arm(Deadline::new(Ticks(10)), test_id(2)),
            Err(TimerError::Closed)
        );

        let mut t2 = TimerObject::new(test_id(2));
        t2.arm(Deadline::new(Ticks(10)), test_id(3)).unwrap();
        let effects2 = t2.close();
        assert!(effects2.cancel_queue);
    }

    #[test]
    fn timer_queue_ordering_and_capacity() {
        let mut q = TimerQueue::try_new(2).unwrap();

        let e1 = TimerEntry {
            deadline: Deadline::new(Ticks(200)),
            timer_id: test_id(1),
        };
        let e2 = TimerEntry {
            deadline: Deadline::new(Ticks(100)),
            timer_id: test_id(2),
        };
        let e3 = TimerEntry {
            deadline: Deadline::new(Ticks(300)),
            timer_id: test_id(3),
        };

        assert_eq!(q.insert(e1), Ok(()));
        assert_eq!(q.insert(e2), Ok(()));
        assert_eq!(q.insert(e3), Err(TimerError::QueueFull));

        // Ensure e2 (deadline 100) is before e1 (deadline 200)
        assert_eq!(q.entries[0], e2);
        assert_eq!(q.entries[1], e1);
    }

    #[test]
    fn timer_queue_advance_and_remove() {
        let mut q = TimerQueue::try_new(3).unwrap();

        q.insert(TimerEntry {
            deadline: Deadline::new(Ticks(100)),
            timer_id: test_id(1),
        })
        .unwrap();
        q.insert(TimerEntry {
            deadline: Deadline::new(Ticks(200)),
            timer_id: test_id(2),
        })
        .unwrap();
        q.insert(TimerEntry {
            deadline: Deadline::new(Ticks(300)),
            timer_id: test_id(3),
        })
        .unwrap();

        q.remove(test_id(2));
        assert_eq!(q.entries.len(), 2);

        let mut fired = alloc::vec::Vec::new();
        q.advance_to(Ticks(150), |id| fired.push(id));
        assert_eq!(fired, vec![test_id(1)]);
        assert_eq!(q.entries.len(), 1);

        q.advance_to(Ticks(350), |id| fired.push(id));
        assert_eq!(fired, vec![test_id(1), test_id(3)]);
        assert_eq!(q.entries.len(), 0);
    }
}
