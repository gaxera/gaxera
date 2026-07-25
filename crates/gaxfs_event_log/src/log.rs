//! Authoritative `GaxFsEventLog` Engine (`EventProvider`)
//!
//! Provides the primary persistent Event Log stream implementing the `EventProvider` trait.

use crate::checkpoint::CheckpointTracker;
use crate::ring_buffer::{DEFAULT_RING_CAPACITY, EventRingBuffer};
use gaxfs_types::{CapabilityHandle, EventError, EventProvider, GaxFsEventRecord};
use std::sync::Mutex;

/// GaxFS Event Log Service implementation
pub struct GaxFsEventLog {
    ring_buffer: Mutex<EventRingBuffer>,
    checkpoint_tracker: Mutex<CheckpointTracker>,
    next_subscriber_id: Mutex<u64>,
}

impl GaxFsEventLog {
    pub fn new() -> Self {
        Self {
            ring_buffer: Mutex::new(EventRingBuffer::new(DEFAULT_RING_CAPACITY)),
            checkpoint_tracker: Mutex::new(CheckpointTracker::new()),
            next_subscriber_id: Mutex::new(1),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            ring_buffer: Mutex::new(EventRingBuffer::new(capacity)),
            checkpoint_tracker: Mutex::new(CheckpointTracker::new()),
            next_subscriber_id: Mutex::new(1),
        }
    }
}

impl Default for GaxFsEventLog {
    fn default() -> Self {
        Self::new()
    }
}

impl EventProvider for GaxFsEventLog {
    fn subscribe(&self, subscriber_cap: &CapabilityHandle) -> Result<u64, EventError> {
        if subscriber_cap.is_revoked() {
            return Err(EventError::SubscriberNotAuthorized);
        }

        let mut sub_id_guard = self.next_subscriber_id.lock().unwrap();
        let sub_id = *sub_id_guard;
        *sub_id_guard += 1;

        let mut tracker = self.checkpoint_tracker.lock().unwrap();
        tracker.update_checkpoint(sub_id, 0);

        Ok(sub_id)
    }

    fn publish(&mut self, record: &GaxFsEventRecord) -> Result<(), EventError> {
        let mut ring = self.ring_buffer.lock().unwrap();
        ring.push(*record);
        Ok(())
    }

    fn replay(
        &self,
        from_sequence: u64,
        callback: &mut dyn FnMut(&GaxFsEventRecord) -> Result<(), EventError>,
    ) -> Result<u64, EventError> {
        let ring = self.ring_buffer.lock().unwrap();
        let records = ring.read_from(from_sequence, 10000);

        let mut count = 0u64;
        for record in &records {
            callback(record)?;
            count += 1;
        }

        Ok(count)
    }

    fn checkpoint(&mut self, subscriber_id: u64, checkpoint_seq: u64) -> Result<(), EventError> {
        let mut tracker = self.checkpoint_tracker.lock().unwrap();
        tracker.update_checkpoint(subscriber_id, checkpoint_seq);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaxfs_types::{CapabilitySpace, GaxFsEventType, GaxFsRights, GaxObjectId};

    #[test]
    fn test_subscriber_registration_and_event_publishing() {
        let mut log = GaxFsEventLog::new();
        let cap =
            CapabilityHandle::new_root(1, CapabilitySpace(1), GaxObjectId::NIL, GaxFsRights::READ);

        let sub_id = log.subscribe(&cap).expect("Subscribe must succeed");
        assert_eq!(sub_id, 1);

        let obj_id = GaxObjectId::new_v7(100, 1, 1);
        let rec = GaxFsEventRecord::new(
            0,
            1000,
            GaxFsEventType::ObjectCreated,
            obj_id,
            1,
            1,
            [0u8; 32],
            0,
        );

        log.publish(&rec).expect("Publish must succeed");

        let mut replayed = Vec::new();
        log.replay(1, &mut |r| {
            replayed.push(*r);
            Ok(())
        })
        .unwrap();

        assert_eq!(replayed.len(), 1);
        assert_eq!(replayed[0].target_object, obj_id);
    }

    #[test]
    fn test_subscriber_self_healing_rehydration() {
        let mut log = GaxFsEventLog::new();
        let cap =
            CapabilityHandle::new_root(1, CapabilitySpace(1), GaxObjectId::NIL, GaxFsRights::READ);

        let sub_id = log.subscribe(&cap).unwrap();

        // Publish 100 events
        for i in 1..=100 {
            let obj_id = GaxObjectId::new_v7(i * 10, 1, 1);
            let rec = GaxFsEventRecord::new(
                0,
                i * 100,
                GaxFsEventType::ObjectModified,
                obj_id,
                1,
                1,
                [0u8; 32],
                0,
            );
            log.publish(&rec).unwrap();
        }

        // Subscriber processes up to sequence 60, then crashes
        log.checkpoint(sub_id, 60).unwrap();

        // Subscriber restarts, recovers checkpoint (60), replays from sequence 61
        let mut rehydrated = Vec::new();
        log.replay(61, &mut |r| {
            rehydrated.push(r.sequence_id);
            Ok(())
        })
        .unwrap();

        assert_eq!(rehydrated.len(), 40);
        assert_eq!(rehydrated[0], 61);
        assert_eq!(rehydrated[39], 100);
    }

    #[test]
    fn test_ring_buffer_multi_wrap_around_and_pruning() {
        let mut log = GaxFsEventLog::with_capacity(100); // Small capacity ring
        let cap =
            CapabilityHandle::new_root(1, CapabilitySpace(1), GaxObjectId::NIL, GaxFsRights::READ);

        let _sub_id = log.subscribe(&cap).unwrap();

        // Publish 250 events (wrapping ring buffer 2.5 times)
        for i in 1..=250 {
            let obj_id = GaxObjectId::new_v7(i * 10, 1, 1);
            let rec = GaxFsEventRecord::new(
                0,
                i * 100,
                GaxFsEventType::ObjectModified,
                obj_id,
                1,
                1,
                [0u8; 32],
                0,
            );
            log.publish(&rec).unwrap();
        }

        // Replay from sequence 201 to 250 (last 50 events in buffer)
        let mut replayed = Vec::new();
        log.replay(201, &mut |r| {
            replayed.push(r.sequence_id);
            Ok(())
        })
        .unwrap();

        assert_eq!(replayed.len(), 50);
        assert_eq!(replayed[0], 201);
        assert_eq!(replayed[49], 250);
    }
}
