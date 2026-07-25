//! GaxFS Core Storage Engine (`gax_storage_engine`)
//!
//! Authoritative, crash-consistent Copy-on-Write storage engine featuring:
//! - Dual-superblock generation commits (`SuperblockHeader`, `DualSuperblockManager`).
//! - Flash erase-block aligned extent allocation (`ExtentAllocator`, `ExtentDescriptor`).
//! - Authoritative Object Header serialization (`GaxFsObjectHeader`) with metadata & graph links.
//! - Private storage engine journaling (`StorageJournal`).
//! - 256-bit cryptographic integrity checksum verification.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod engine;
pub mod extent_allocator;
pub mod integrity;
pub mod journal;
pub mod object_header;
pub mod superblock;

pub use engine::GaxStorageEngine;
pub use extent_allocator::{ExtentAllocator, ExtentDescriptor};
pub use integrity::{compute_checksum, verify_checksum};
pub use journal::{JournalRecord, StorageJournal};
pub use object_header::{GaxFsObjectHeader, RelationshipEdge, RelationshipKind};
pub use superblock::{
    DualSuperblockManager, SUPERBLOCK_0_OFFSET, SUPERBLOCK_1_OFFSET, SuperblockHeader,
};

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;
    use gaxfs_types::{GaxObjectId, StorageDeviceProvider, StorageError};

    /// In-Memory RAM Storage Device implementation for unit testing
    pub struct RamStorageDevice {
        blocks: Vec<Vec<u8>>,
        block_size: u32,
    }

    impl RamStorageDevice {
        pub fn new(num_blocks: usize, block_size: u32) -> Self {
            Self {
                blocks: vec![vec![0u8; block_size as usize]; num_blocks],
                block_size,
            }
        }
    }

    impl StorageDeviceProvider for RamStorageDevice {
        fn read_blocks(
            &self,
            start_block: u64,
            num_blocks: u32,
            buf: &mut [u8],
        ) -> Result<(), StorageError> {
            let start = start_block as usize;
            let count = num_blocks as usize;
            if start + count > self.blocks.len() {
                return Err(StorageError::OutOfBounds {
                    requested: (start + count) as u64,
                    max: self.blocks.len() as u64,
                });
            }

            let bs = self.block_size as usize;
            for i in 0..count {
                buf[i * bs..(i + 1) * bs].copy_from_slice(&self.blocks[start + i]);
            }
            Ok(())
        }

        fn write_blocks(
            &mut self,
            start_block: u64,
            num_blocks: u32,
            buf: &[u8],
        ) -> Result<(), StorageError> {
            let start = start_block as usize;
            let count = num_blocks as usize;
            if start + count > self.blocks.len() {
                return Err(StorageError::OutOfBounds {
                    requested: (start + count) as u64,
                    max: self.blocks.len() as u64,
                });
            }

            let bs = self.block_size as usize;
            for i in 0..count {
                self.blocks[start + i].copy_from_slice(&buf[i * bs..(i + 1) * bs]);
            }
            Ok(())
        }

        fn flush_cache(&mut self) -> Result<(), StorageError> {
            Ok(())
        }

        fn block_size(&self) -> u32 {
            self.block_size
        }

        fn total_blocks(&self) -> u64 {
            self.blocks.len() as u64
        }
    }

    #[test]
    fn test_storage_engine_format_write_read_cycle() {
        let dev = RamStorageDevice::new(1024, 512);
        let root_id = GaxObjectId::new_v7(1000, 1, 1);

        let mut engine =
            GaxStorageEngine::format(dev, root_id).expect("Formatting device must succeed");
        assert_eq!(engine.active_superblock().generation, 1);

        let file_id = GaxObjectId::new_v7(2000, 2, 2);
        let payload = b"GaxFS Core Storage Engine End-to-End Object Payload Data!";
        let attributes = vec![("type".to_string(), "code".to_string())];

        let header = engine
            .write_object(file_id, payload, attributes)
            .expect("Object write must succeed");

        assert_eq!(header.object_id, file_id);
        assert_eq!(
            engine.active_superblock().generation,
            2,
            "Generation must increment on write"
        );

        let read_payload = engine
            .read_object(&header)
            .expect("Object read must succeed");
        assert_eq!(read_payload, payload);
    }

    #[test]
    fn test_hundred_generations_commit_rollover() {
        let dev = RamStorageDevice::new(4096, 512);
        let root_id = GaxObjectId::new_v7(1000, 1, 1);
        let mut engine = GaxStorageEngine::format(dev, root_id).unwrap();

        for i in 1..=100 {
            let obj_id = GaxObjectId::new_v7(1000 + i, 1, i);
            let payload = vec![(i % 255) as u8; 256];
            let header = engine.write_object(obj_id, &payload, vec![]).unwrap();

            assert_eq!(engine.active_superblock().generation, i + 1);
            let recovered_payload = engine.read_object(&header).unwrap();
            assert_eq!(recovered_payload, payload);
        }
    }

    #[test]
    fn test_bitrot_payload_checksum_scrubbing() {
        let dev = RamStorageDevice::new(1024, 512);
        let root_id = GaxObjectId::new_v7(1000, 1, 1);
        let mut engine = GaxStorageEngine::format(dev, root_id).unwrap();

        let obj_id = GaxObjectId::new_v7(2000, 2, 2);
        let payload = b"Sensitive Integrity-Scrubbed Object Data Payload";
        let header = engine.write_object(obj_id, payload, vec![]).unwrap();

        // Corrupt serialized checksum bytes to simulate storage bitrot
        let mut buf = header.serialize();
        let last_idx = buf.len() - 1;
        buf[last_idx] ^= 0xFF;

        let res = GaxFsObjectHeader::deserialize(&buf);
        assert!(
            matches!(res, Err(StorageError::ChecksumMismatch)),
            "Bitrot corruption must be detected during deserialization"
        );
    }
}
