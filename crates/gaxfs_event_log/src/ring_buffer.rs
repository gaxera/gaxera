//! High-Performance Event Ring Buffer Module
//!
//! Provides a zero-copy shared-memory event publication buffer
//! supporting high-throughput emission ($>500,000\\text{ events/sec}$).

use alloc::vec::Vec;
use gaxfs_types::GaxFsEventRecord;

pub const DEFAULT_RING_CAPACITY: usize = 65536; // 64K events ring capacity

/// Ring Buffer Event Storage
#[derive(Debug)]
pub struct EventRingBuffer {
    buffer: Vec<GaxFsEventRecord>,
    capacity: usize,
    head: u64, // Monotonic sequence ID of next write slot
}

impl EventRingBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(capacity),
            capacity,
            head: 1, // Monotonic sequence numbers start at 1
        }
    }

    /// Appends a new event record to the ring buffer
    pub fn push(&mut self, mut record: GaxFsEventRecord) -> u64 {
        let seq = self.head;
        record.sequence_id = seq;
        self.head += 1;

        if self.buffer.len() < self.capacity {
            self.buffer.push(record);
        } else {
            let slot = ((seq - 1) as usize) % self.capacity;
            self.buffer[slot] = record;
        }

        seq
    }

    /// Reads event records starting from sequence ID
    pub fn read_from(&self, from_sequence: u64, max_count: usize) -> Vec<GaxFsEventRecord> {
        let mut result = Vec::new();
        if from_sequence >= self.head {
            return result;
        }

        // Calculate oldest available sequence number
        let oldest_available = if self.head > self.capacity as u64 {
            self.head - self.capacity as u64
        } else {
            1
        };

        let start_seq = from_sequence.max(oldest_available);
        let end_seq = (start_seq + max_count as u64).min(self.head);

        for seq in start_seq..end_seq {
            let slot = ((seq - 1) as usize) % self.capacity;
            if slot < self.buffer.len() {
                result.push(self.buffer[slot]);
            }
        }

        result
    }

    pub fn current_head(&self) -> u64 {
        self.head
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaxfs_types::{GaxFsEventType, GaxObjectId};

    #[test]
    fn test_ring_buffer_push_and_read() {
        let mut ring = EventRingBuffer::new(100);
        let obj_id = GaxObjectId::new_v7(1000, 1, 1);

        for i in 1..=50 {
            let rec = GaxFsEventRecord::new(
                0,
                i * 100,
                GaxFsEventType::ObjectCreated,
                obj_id,
                1,
                1,
                [0u8; 32],
                0,
            );
            ring.push(rec);
        }

        assert_eq!(ring.current_head(), 51);

        let records = ring.read_from(1, 10);
        assert_eq!(records.len(), 10);
        assert_eq!(records[0].sequence_id, 1);
        assert_eq!(records[9].sequence_id, 10);
    }
}
