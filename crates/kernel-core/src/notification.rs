use crate::object::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationError {
    Closed,
}

/// Pure Notification Signal State Machine (ADR 0013 Compliant).
///
/// Notification holds only `signals: u32` bitfield state. It maintains zero
/// waiter/subscriber lists, ensuring fixed-size footprint and zero fast-path heap allocations.
#[derive(Clone, Debug)]
pub struct Notification {
    id: ObjectId,
    signals: u32,
    waiting_thread: Option<ObjectId>,
    closed: bool,
}

impl Notification {
    pub fn new(id: ObjectId) -> Self {
        Self {
            id,
            signals: 0,
            waiting_thread: None,
            closed: false,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn signals(&self) -> u32 {
        self.signals
    }

    pub fn waiting_thread(&self) -> Option<ObjectId> {
        self.waiting_thread
    }

    /// Atomically performs bitwise OR to post signal bits. Returns waiting thread to wake if present.
    pub fn signal(&mut self, active_signals: u32) -> Option<ObjectId> {
        if !self.closed {
            self.signals |= active_signals;
            if self.signals != 0 {
                return self.waiting_thread.take();
            }
        }
        None
    }

    /// Attempts to take signals or records thread as waiting if none present.
    pub fn wait(&mut self, thread: ObjectId) -> Result<Result<u32, ObjectId>, NotificationError> {
        if self.closed {
            return Err(NotificationError::Closed);
        }
        if self.signals != 0 {
            Ok(Ok(self.take_signals()))
        } else {
            self.waiting_thread = Some(thread);
            Ok(Err(thread))
        }
    }

    /// Clears specified signal bits and returns previous value.
    pub fn clear(&mut self, mask: u32) -> u32 {
        let old = self.signals & mask;
        self.signals &= !mask;
        old
    }

    /// Returns and resets all signal bits.
    pub fn take_signals(&mut self) -> u32 {
        let s = self.signals;
        self.signals = 0;
        s
    }

    pub fn close(&mut self) -> Option<ObjectId> {
        self.closed = true;
        self.signals = 0;
        self.waiting_thread.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn notification_signal_coalescing_and_take() {
        let mut notif = Notification::new(test_id(1));
        assert_eq!(notif.signals(), 0);

        notif.signal(0b0001);
        notif.signal(0b0010);
        assert_eq!(notif.signals(), 0b0011);

        assert_eq!(notif.clear(0b0001), 0b0001);
        assert_eq!(notif.signals(), 0b0010);

        assert_eq!(notif.take_signals(), 0b0010);
        assert_eq!(notif.signals(), 0);
    }

    #[test]
    fn notification_close_effects() {
        let mut notif = Notification::new(test_id(1));
        notif.signal(0b1111);
        let woke = notif.close();
        assert_eq!(woke, None);
        assert_eq!(notif.signals(), 0);

        // Signalling closed notification ignored
        assert_eq!(notif.signal(0b0001), None);
        assert_eq!(notif.signals(), 0);
    }

    #[test]
    fn notification_wait_with_pending_signals_returns_immediately() {
        let mut notif = Notification::new(test_id(1));
        let thread = test_id(10);
        notif.signal(0b0101);

        assert_eq!(notif.wait(thread), Ok(Ok(0b0101)));
        assert_eq!(notif.signals(), 0);
    }

    #[test]
    fn notification_wait_without_signals_records_waiter() {
        let mut notif = Notification::new(test_id(1));
        let thread = test_id(10);

        assert_eq!(notif.wait(thread), Ok(Err(thread)));
        assert_eq!(notif.waiting_thread(), Some(thread));
    }

    #[test]
    fn notification_signal_wakes_waiting_thread() {
        let mut notif = Notification::new(test_id(1));
        let thread = test_id(10);

        assert_eq!(notif.wait(thread), Ok(Err(thread)));
        let woken = notif.signal(0b1000);
        assert_eq!(woken, Some(thread));
        assert_eq!(notif.waiting_thread(), None);
    }
}
