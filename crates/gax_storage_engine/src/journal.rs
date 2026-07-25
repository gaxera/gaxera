//! Private Storage Journal Module (`gax_storage_journal`)
//!
//! Provides transactional journaling owned privately by `gax_storage_engine`
//! for atomic multi-block transaction commits and rollback recovery.

use crate::integrity::{compute_checksum, verify_checksum};
use alloc::vec::Vec;
use gaxfs_types::StorageError;

/// Journal Transaction Record
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JournalRecord {
    pub transaction_id: u64,
    pub target_block: u64,
    pub block_data: Vec<u8>,
    pub checksum: [u8; 32],
}

impl JournalRecord {
    pub fn new(transaction_id: u64, target_block: u64, block_data: Vec<u8>) -> Self {
        let mut rec = Self {
            transaction_id,
            target_block,
            block_data,
            checksum: [0u8; 32],
        };
        rec.update_checksum();
        rec
    }

    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&self.transaction_id.to_le_bytes());
        buf.extend_from_slice(&self.target_block.to_le_bytes());
        buf.extend_from_slice(&(self.block_data.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.block_data);

        let checksum = compute_checksum(&buf);
        buf.extend_from_slice(&checksum);
        buf
    }

    pub fn deserialize(buf: &[u8]) -> Result<Self, StorageError> {
        if buf.len() < 20 + 32 {
            return Err(StorageError::OutOfBounds {
                requested: buf.len() as u64,
                max: 52,
            });
        }

        let data_len = buf.len() - 32;
        let body = &buf[..data_len];
        let expected_checksum: &[u8; 32] = buf[data_len..].try_into().unwrap();

        if !verify_checksum(body, expected_checksum) {
            return Err(StorageError::ChecksumMismatch);
        }

        let transaction_id = u64::from_le_bytes(body[0..8].try_into().unwrap());
        let target_block = u64::from_le_bytes(body[8..16].try_into().unwrap());
        let block_len = u32::from_le_bytes(body[16..20].try_into().unwrap()) as usize;

        let block_data = body[20..20 + block_len].to_vec();

        Ok(Self {
            transaction_id,
            target_block,
            block_data,
            checksum: *expected_checksum,
        })
    }

    pub fn update_checksum(&mut self) {
        let serialized = self.serialize();
        let data_len = serialized.len() - 32;
        self.checksum.copy_from_slice(&serialized[data_len..]);
    }
}

/// Private Storage Engine Journal Manager
#[derive(Debug)]
pub struct StorageJournal {
    pub start_block: u64,
    pub total_blocks: u32,
    pub current_tx_id: u64,
}

impl StorageJournal {
    pub fn new(start_block: u64, total_blocks: u32) -> Self {
        Self {
            start_block,
            total_blocks,
            current_tx_id: 1,
        }
    }

    pub fn begin_transaction(&mut self) -> u64 {
        let tx_id = self.current_tx_id;
        self.current_tx_id += 1;
        tx_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_record_serialization() {
        let rec = JournalRecord::new(42, 100, vec![0xAA; 512]);
        let serialized = rec.serialize();
        let deserialized = JournalRecord::deserialize(&serialized)
            .expect("Journal record deserialization must succeed");

        assert_eq!(deserialized.transaction_id, 42);
        assert_eq!(deserialized.target_block, 100);
        assert_eq!(deserialized.block_data.len(), 512);
        assert_eq!(deserialized.block_data[0], 0xAA);
    }
}
