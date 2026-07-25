//! Primary Storage Engine Facade (`GaxStorageEngine`)
//!
//! Integrates dual-superblock management, flash extent allocation,
//! object header serialization, and journaled recovery over a `StorageDeviceProvider`.

use crate::extent_allocator::{ExtentAllocator, ExtentDescriptor};
use crate::object_header::GaxFsObjectHeader;
use crate::superblock::{
    DualSuperblockManager, SUPERBLOCK_0_OFFSET, SUPERBLOCK_1_OFFSET, SuperblockHeader,
};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use gaxfs_types::{GaxObjectId, StorageDeviceProvider, StorageError};

/// GaxFS Core Storage Engine Manager
pub struct GaxStorageEngine<D: StorageDeviceProvider> {
    device: D,
    superblock_mgr: DualSuperblockManager,
    extent_allocator: ExtentAllocator,
}

impl<D: StorageDeviceProvider> GaxStorageEngine<D> {
    /// Formats a block device with initial dual superblocks and empty root object
    pub fn format(mut device: D, root_id: GaxObjectId) -> Result<Self, StorageError> {
        let block_size = device.block_size();
        let total_blocks = device.total_blocks();

        if total_blocks < 128 {
            return Err(StorageError::OutOfBounds {
                requested: 128,
                max: total_blocks,
            });
        }

        // Initialize Superblock 0 (generation 1)
        let sb0 = SuperblockHeader::new(1, block_size, total_blocks, root_id, 2, 64);
        let mut sb0_buf = vec![0u8; block_size as usize];
        sb0.serialize(&mut sb0_buf);
        device.write_blocks(SUPERBLOCK_0_OFFSET, 1, &sb0_buf)?;

        // Initialize Superblock 1 (generation 1)
        let sb1 = SuperblockHeader::new(1, block_size, total_blocks, root_id, 2, 64);
        let mut sb1_buf = vec![0u8; block_size as usize];
        sb1.serialize(&mut sb1_buf);
        device.write_blocks(SUPERBLOCK_1_OFFSET, 1, &sb1_buf)?;

        device.flush_cache()?;

        let superblock_mgr = DualSuperblockManager::recover_active(&sb0_buf, &sb1_buf)?;
        let extent_allocator = ExtentAllocator::new(total_blocks, 8); // Align to 8 blocks

        Ok(Self {
            device,
            superblock_mgr,
            extent_allocator,
        })
    }

    /// Mounts an existing storage device, performing dual-superblock crash recovery
    pub fn mount(device: D) -> Result<Self, StorageError> {
        let block_size = device.block_size();
        let total_blocks = device.total_blocks();

        let mut sb0_buf = vec![0u8; block_size as usize];
        let mut sb1_buf = vec![0u8; block_size as usize];

        device.read_blocks(SUPERBLOCK_0_OFFSET, 1, &mut sb0_buf)?;
        device.read_blocks(SUPERBLOCK_1_OFFSET, 1, &mut sb1_buf)?;

        let superblock_mgr = DualSuperblockManager::recover_active(&sb0_buf, &sb1_buf)?;
        let mut extent_allocator = ExtentAllocator::new(total_blocks, 8);

        // Mark superblock and journal blocks as allocated
        let _ = extent_allocator.allocate_extent(2 + 64);

        Ok(Self {
            device,
            superblock_mgr,
            extent_allocator,
        })
    }

    /// Writes an object payload and header to persistent storage
    pub fn write_object(
        &mut self,
        object_id: GaxObjectId,
        payload: &[u8],
        attributes: Vec<(String, String)>,
    ) -> Result<GaxFsObjectHeader, StorageError> {
        let block_size = self.device.block_size() as usize;
        let num_blocks = payload.len().div_ceil(block_size) as u32;

        let extent = if num_blocks > 0 {
            self.extent_allocator.allocate_extent(num_blocks)?
        } else {
            ExtentDescriptor {
                start_block: 0,
                num_blocks: 0,
            }
        };

        if num_blocks > 0 {
            let mut padded_payload = payload.to_vec();
            padded_payload.resize(num_blocks as usize * block_size, 0);
            self.device
                .write_blocks(extent.start_block, num_blocks, &padded_payload)?;
        }

        let mut header = GaxFsObjectHeader::new(object_id, payload.len() as u64);
        if num_blocks > 0 {
            header.extents.push(extent);
        }
        header.attributes = attributes;
        header.update_checksum();

        // Write header to storage
        let header_bytes = header.serialize();
        let header_blocks = header_bytes.len().div_ceil(block_size) as u32;
        let header_extent = self.extent_allocator.allocate_extent(header_blocks)?;
        let mut padded_header = header_bytes;
        padded_header.resize(header_blocks as usize * block_size, 0);
        self.device
            .write_blocks(header_extent.start_block, header_blocks, &padded_header)?;

        self.commit_generation(object_id)?;

        Ok(header)
    }

    /// Reads object payload extents from storage given an Object Header
    pub fn read_object(&self, header: &GaxFsObjectHeader) -> Result<Vec<u8>, StorageError> {
        let block_size = self.device.block_size() as usize;
        let mut payload = Vec::with_capacity(header.payload_size as usize);

        for extent in &header.extents {
            let mut buf = vec![0u8; extent.num_blocks as usize * block_size];
            self.device
                .read_blocks(extent.start_block, extent.num_blocks, &mut buf)?;
            payload.extend_from_slice(&buf);
        }

        payload.truncate(header.payload_size as usize);
        Ok(payload)
    }

    /// Commits a new transaction generation to alternating dual superblocks
    fn commit_generation(&mut self, new_root_id: GaxObjectId) -> Result<(), StorageError> {
        let next_generation = self.superblock_mgr.active_superblock.generation + 1;
        let target_sb_index = if self.superblock_mgr.active_index == 0 {
            1
        } else {
            0
        };
        let target_block_offset = if target_sb_index == 0 {
            SUPERBLOCK_0_OFFSET
        } else {
            SUPERBLOCK_1_OFFSET
        };

        let block_size = self.device.block_size();
        let total_blocks = self.device.total_blocks();

        let new_sb = SuperblockHeader::new(
            next_generation,
            block_size,
            total_blocks,
            new_root_id,
            self.superblock_mgr.active_superblock.journal_start_block,
            self.superblock_mgr.active_superblock.journal_block_count,
        );

        let mut buf = vec![0u8; block_size as usize];
        new_sb.serialize(&mut buf);

        self.device.write_blocks(target_block_offset, 1, &buf)?;
        self.device.flush_cache()?;

        self.superblock_mgr.active_superblock = new_sb;
        self.superblock_mgr.active_index = target_sb_index;

        Ok(())
    }

    pub fn active_superblock(&self) -> &SuperblockHeader {
        &self.superblock_mgr.active_superblock
    }
}
