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
    closed: bool,
}

impl Notification {
    pub fn new(id: ObjectId) -> Self {
        Self {
            id,
            signals: 0,
            closed: false,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn signals(&self) -> u32 {
        self.signals
    }

    /// Atomically performs bitwise OR to post signal bits.
    pub fn signal(&mut self, active_signals: u32) {
        if !self.closed {
            self.signals |= active_signals;
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

    pub fn close(&mut self) {
        self.closed = true;
        self.signals = 0;
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
        notif.close();
        assert_eq!(notif.signals(), 0);

        // Signalling closed notification ignored
        notif.signal(0b0001);
        assert_eq!(notif.signals(), 0);
    }
}
