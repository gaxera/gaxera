//! Subscriber Checkpoint & State Rehydration Module
//!
//! Manages `EventCheckpointMarker` tracking per subscriber to enable log compaction
//! and deterministic index self-healing rehydration.

use alloc::collections::BTreeMap;

/// Subscriber Checkpoint Manager
#[derive(Debug, Default)]
pub struct CheckpointTracker {
    checkpoints: BTreeMap<u64, u64>, // subscriber_id -> checkpoint_sequence_id
}

impl CheckpointTracker {
    pub fn new() -> Self {
        Self {
            checkpoints: BTreeMap::new(),
        }
    }

    /// Records or updates a subscriber checkpoint sequence
    pub fn update_checkpoint(&mut self, subscriber_id: u64, checkpoint_seq: u64) {
        self.checkpoints.insert(subscriber_id, checkpoint_seq);
    }

    /// Returns the lowest checkpoint sequence across all active subscribers
    pub fn lowest_checkpoint(&self) -> Option<u64> {
        self.checkpoints.values().cloned().min()
    }

    /// Removes a subscriber from checkpoint tracking
    pub fn remove_subscriber(&mut self, subscriber_id: u64) -> Option<u64> {
        self.checkpoints.remove(&subscriber_id)
    }

    /// Returns the checkpoint sequence for a specific subscriber
    pub fn get_checkpoint(&self, subscriber_id: u64) -> Option<u64> {
        self.checkpoints.get(&subscriber_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checkpoint_tracking_and_lowest_sequence() {
        let mut tracker = CheckpointTracker::new();

        tracker.update_checkpoint(1, 100);
        tracker.update_checkpoint(2, 50);
        tracker.update_checkpoint(3, 200);

        assert_eq!(tracker.lowest_checkpoint(), Some(50));

        tracker.update_checkpoint(2, 120);
        assert_eq!(tracker.lowest_checkpoint(), Some(100));

        tracker.remove_subscriber(1);
        assert_eq!(tracker.lowest_checkpoint(), Some(120));
    }
}
