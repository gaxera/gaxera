use crate::object::ObjectId;
use alloc::vec::Vec;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationError {
    Busy,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationEffect {
    Block,
    Wake(ObjectId),
}

#[derive(Clone, Debug)]
enum NotificationState {
    Idle { pending_bits: u64 },
    Waiting { waiter: ObjectId },
}

pub struct Notification {
    #[allow(dead_code)]
    id: ObjectId,
    state: NotificationState,
    closed: bool,
}

impl Notification {
    pub fn new(id: ObjectId) -> Self {
        Self {
            id,
            state: NotificationState::Idle { pending_bits: 0 },
            closed: false,
        }
    }

    pub fn signal(&mut self, bits: u64) -> Result<Option<NotificationEffect>, NotificationError> {
        if self.closed {
            return Err(NotificationError::Closed);
        }

        if bits == 0 {
            return Ok(None);
        }

        match self.state {
            NotificationState::Idle {
                ref mut pending_bits,
            } => {
                *pending_bits |= bits;
                Ok(None)
            }
            NotificationState::Waiting { waiter } => {
                self.state = NotificationState::Idle { pending_bits: bits };
                Ok(Some(NotificationEffect::Wake(waiter)))
            }
        }
    }

    pub fn wait(
        &mut self,
        waiter: ObjectId,
    ) -> Result<Result<u64, NotificationEffect>, NotificationError> {
        if self.closed {
            return Err(NotificationError::Closed);
        }

        match self.state {
            NotificationState::Idle {
                ref mut pending_bits,
            } => {
                let bits = *pending_bits;
                if bits != 0 {
                    *pending_bits = 0;
                    Ok(Ok(bits))
                } else {
                    self.state = NotificationState::Waiting { waiter };
                    Ok(Err(NotificationEffect::Block))
                }
            }
            NotificationState::Waiting { .. } => Err(NotificationError::Busy),
        }
    }

    pub fn take_pending(&mut self) -> u64 {
        match &mut self.state {
            NotificationState::Idle { pending_bits } => {
                let bits = *pending_bits;
                *pending_bits = 0;
                bits
            }
            _ => 0,
        }
    }

    pub fn close(&mut self) -> Vec<ObjectId> {
        self.closed = true;
        let mut woke_threads = Vec::new();

        if let NotificationState::Waiting { waiter } = self.state {
            woke_threads.push(waiter);
        }

        self.state = NotificationState::Idle { pending_bits: 0 };
        woke_threads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn notification_signal_before_wait() {
        let mut notif = Notification::new(test_id(1));

        assert_eq!(notif.signal(0x1), Ok(None));
        assert_eq!(notif.signal(0x2), Ok(None));

        assert_eq!(notif.wait(test_id(2)), Ok(Ok(0x3)));
    }

    #[test]
    fn notification_wait_before_signal() {
        let mut notif = Notification::new(test_id(1));
        let waiter = test_id(2);

        assert_eq!(notif.wait(waiter), Ok(Err(NotificationEffect::Block)));

        assert_eq!(
            notif.signal(0x4),
            Ok(Some(NotificationEffect::Wake(waiter)))
        );

        assert_eq!(notif.take_pending(), 0x4);
        assert_eq!(notif.take_pending(), 0x0);
    }

    #[test]
    fn notification_bit_coalescing() {
        let mut notif = Notification::new(test_id(1));

        assert_eq!(notif.signal(0b0101), Ok(None));
        assert_eq!(notif.signal(0b1010), Ok(None));
        assert_eq!(notif.signal(0b1100), Ok(None));

        assert_eq!(notif.wait(test_id(2)), Ok(Ok(0b1111)));
    }

    #[test]
    fn notification_one_waiter_limit() {
        let mut notif = Notification::new(test_id(1));
        let waiter1 = test_id(2);
        let waiter2 = test_id(3);

        assert_eq!(notif.wait(waiter1), Ok(Err(NotificationEffect::Block)));
        assert_eq!(notif.wait(waiter2), Err(NotificationError::Busy));
    }

    #[test]
    fn notification_close_effects() {
        let mut notif1 = Notification::new(test_id(1));
        assert_eq!(notif1.wait(test_id(2)), Ok(Err(NotificationEffect::Block)));

        let woke = notif1.close();
        assert_eq!(woke, alloc::vec![test_id(2)]);

        assert_eq!(notif1.signal(0x1), Err(NotificationError::Closed));
        assert_eq!(notif1.wait(test_id(3)), Err(NotificationError::Closed));
    }
}
