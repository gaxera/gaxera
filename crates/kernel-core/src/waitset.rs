use crate::object::ObjectId;
use alloc::vec::Vec;
use gaxera_abi::WaitSetEvent;

pub const MAX_WAITSET_SUBSCRIPTIONS: usize = 64;
pub const MAX_WAITSET_EVENTS: usize = 128;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscription {
    pub object: ObjectId,
    pub cookie: u64,
    pub signals: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WaitSetError {
    Full,
    NotFound,
    AlreadyRegistered,
    Closed,
}

#[derive(Clone, Debug)]
pub struct WaitSet {
    id: ObjectId,
    subscriptions: Vec<Subscription>,
    ready_events: Vec<WaitSetEvent>,
    waiting_thread: Option<ObjectId>,
    closed: bool,
}

impl WaitSet {
    pub fn new(id: ObjectId) -> Self {
        Self {
            id,
            subscriptions: Vec::new(),
            ready_events: Vec::new(),
            waiting_thread: None,
            closed: false,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn subscription_count(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn pending_event_count(&self) -> usize {
        self.ready_events.len()
    }

    pub fn add_subscription(
        &mut self,
        object: ObjectId,
        cookie: u64,
        signals: u32,
    ) -> Result<(), WaitSetError> {
        if self.closed {
            return Err(WaitSetError::Closed);
        }
        if self.subscriptions.len() >= MAX_WAITSET_SUBSCRIPTIONS {
            return Err(WaitSetError::Full);
        }
        if self.subscriptions.iter().any(|s| s.object == object) {
            return Err(WaitSetError::AlreadyRegistered);
        }
        self.subscriptions.push(Subscription {
            object,
            cookie,
            signals,
        });
        Ok(())
    }

    pub fn remove_subscription(&mut self, object: ObjectId) -> Result<(), WaitSetError> {
        if self.closed {
            return Err(WaitSetError::Closed);
        }
        let target_cookie = self
            .subscriptions
            .iter()
            .find(|s| s.object == object)
            .map(|s| s.cookie);

        let len_before = self.subscriptions.len();
        self.subscriptions.retain(|s| s.object != object);
        if self.subscriptions.len() < len_before {
            if let Some(cookie) = target_cookie {
                self.ready_events.retain(|e| e.cookie != cookie);
            }
            Ok(())
        } else {
            Err(WaitSetError::NotFound)
        }
    }

    pub fn post_event(&mut self, object: ObjectId, active_signals: u32) -> Option<ObjectId> {
        if self.closed {
            return None;
        }

        if let Some(sub) = self.subscriptions.iter().find(|s| s.object == object) {
            let matched_signals = sub.signals & active_signals;
            if matched_signals != 0 {
                if self.ready_events.len() < MAX_WAITSET_EVENTS {
                    self.ready_events.push(WaitSetEvent {
                        cookie: sub.cookie,
                        signals: matched_signals,
                        _reserved: 0,
                    });
                }
                return self.waiting_thread.take();
            }
        }
        None
    }

    pub fn wait(
        &mut self,
        thread: ObjectId,
    ) -> Result<Result<Vec<WaitSetEvent>, ObjectId>, WaitSetError> {
        if self.closed {
            return Err(WaitSetError::Closed);
        }

        if !self.ready_events.is_empty() {
            let events = core::mem::take(&mut self.ready_events);
            Ok(Ok(events))
        } else {
            self.waiting_thread = Some(thread);
            Ok(Err(thread))
        }
    }

    pub fn close(&mut self) -> Option<ObjectId> {
        self.closed = true;
        self.subscriptions.clear();
        self.ready_events.clear();
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
    fn waitset_add_remove_subscription() {
        let mut ws = WaitSet::new(test_id(1));
        let obj1 = test_id(10);
        let obj2 = test_id(20);

        assert_eq!(ws.add_subscription(obj1, 0x100, 0b01), Ok(()));
        assert_eq!(ws.add_subscription(obj2, 0x200, 0b10), Ok(()));
        assert_eq!(ws.subscription_count(), 2);

        // Duplicate rejection
        assert_eq!(
            ws.add_subscription(obj1, 0x100, 0b01),
            Err(WaitSetError::AlreadyRegistered)
        );

        // Remove
        assert_eq!(ws.remove_subscription(obj1), Ok(()));
        assert_eq!(ws.subscription_count(), 1);
        assert_eq!(ws.remove_subscription(obj1), Err(WaitSetError::NotFound));
    }

    #[test]
    fn waitset_post_event_and_atomic_wait() {
        let mut ws = WaitSet::new(test_id(1));
        let ep = test_id(10);
        let notif = test_id(20);
        let thread = test_id(100);

        ws.add_subscription(ep, 0xA1, 0b01).unwrap();
        ws.add_subscription(notif, 0xB2, 0b10).unwrap();

        // 1. Wait when no events ready -> blocks thread
        assert_eq!(ws.wait(thread), Ok(Err(thread)));

        // 2. Event posted on ep -> wakes thread
        let woken = ws.post_event(ep, 0b01);
        assert_eq!(woken, Some(thread));

        // 3. Second event posted on notif
        assert_eq!(ws.post_event(notif, 0b10), None);

        // 4. Wait now delivers both ready events immediately
        let res = ws.wait(thread).unwrap().unwrap();
        assert_eq!(res.len(), 2);
        assert_eq!(res[0].cookie, 0xA1);
        assert_eq!(res[1].cookie, 0xB2);
    }

    #[test]
    fn waitset_remove_subscription_purges_pending_events() {
        let mut ws = WaitSet::new(test_id(1));
        let ep = test_id(10);
        let notif = test_id(20);
        let thread = test_id(100);

        ws.add_subscription(ep, 0xA1, 0b01).unwrap();
        ws.add_subscription(notif, 0xB2, 0b10).unwrap();

        // Post events for both subscriptions
        assert_eq!(ws.post_event(ep, 0b01), None);
        assert_eq!(ws.post_event(notif, 0b10), None);
        assert_eq!(ws.pending_event_count(), 2);

        // Remove ep subscription -> should purge ep event
        assert_eq!(ws.remove_subscription(ep), Ok(()));
        assert_eq!(ws.pending_event_count(), 1);

        // Wait delivers only remaining notif event
        let res = ws.wait(thread).unwrap().unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].cookie, 0xB2);
    }
}
