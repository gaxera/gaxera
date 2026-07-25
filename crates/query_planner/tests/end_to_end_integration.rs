//! End-to-End System Integration Test for Milestone 0.9.3 (GaxFS Platform)
//!
//! Integrates `gaxfs_types`, `gax_storage_engine`, `gaxfs_event_log`,
//! `gaxfs_vector_index`, and `query_planner` into a full end-to-end system flow.

use gax_storage_engine::GaxStorageEngine;
use gaxfs_event_log::GaxFsEventLog;
use gaxfs_types::{
    CapabilityHandle, CapabilitySpace, EventProvider, GaxFsEventRecord, GaxFsEventType,
    GaxFsRights, GaxObjectId, StorageDeviceProvider, StorageError,
};
use gaxfs_vector_index::TurboVecProvider;
use query_planner::{QueryBuilder, QueryPlanner};

/// RAM Storage Device for Integration Testing
pub struct IntegrationRamDevice {
    blocks: Vec<Vec<u8>>,
    block_size: u32,
}

impl IntegrationRamDevice {
    pub fn new(num_blocks: usize, block_size: u32) -> Self {
        Self {
            blocks: vec![vec![0u8; block_size as usize]; num_blocks],
            block_size,
        }
    }
}

impl StorageDeviceProvider for IntegrationRamDevice {
    fn read_blocks(
        &self,
        start_block: u64,
        num_blocks: u32,
        buf: &mut [u8],
    ) -> Result<(), StorageError> {
        let start = start_block as usize;
        let count = num_blocks as usize;
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
fn test_milestone_093_full_system_integration() {
    // 1. Storage Engine Setup
    let dev = IntegrationRamDevice::new(1024, 512);
    let root_id = GaxObjectId::new_v7(1000, 1, 1);
    let mut storage =
        GaxStorageEngine::format(dev, root_id).expect("Storage engine formatting must succeed");

    // 2. Object Write & Storage Generation Commit
    let doc_id = GaxObjectId::new_v7(2000, 2, 2);
    let payload = b"GaxFS End-to-End System Integration Test Payload Data";
    let attributes = vec![("type".to_string(), "document".to_string())];
    let header = storage
        .write_object(doc_id, payload, attributes)
        .expect("Write object must succeed");

    assert_eq!(header.object_id, doc_id);
    assert_eq!(storage.active_superblock().generation, 2);

    // Read back object payload from storage engine
    let read_payload = storage
        .read_object(&header)
        .expect("Read object must succeed");
    assert_eq!(read_payload, payload);

    // 3. Event Publication to OS Event Log
    let mut event_log = GaxFsEventLog::new();
    let root_cap = CapabilityHandle::new_root(1, CapabilitySpace(1), root_id, GaxFsRights::READ);
    let _sub_id = event_log
        .subscribe(&root_cap)
        .expect("Event subscription must succeed");

    let event = GaxFsEventRecord::new(
        0,
        100,
        GaxFsEventType::ObjectCreated,
        doc_id,
        1,
        1,
        [0u8; 32],
        0,
    );
    event_log
        .publish(&event)
        .expect("Event publish must succeed");

    // 4. Vector Index Ingestion
    let mut vector_index = TurboVecProvider::new(4);
    vector_index.insert_vector(doc_id, vec![1.0, 0.0, 0.0, 0.0]);

    // 5. Query Planning & Execution
    let doc_cap = CapabilityHandle::new_root(2, CapabilitySpace(1), doc_id, GaxFsRights::READ);
    let query = QueryBuilder::with_scope(doc_cap)
        .where_similar_to(vec![1.0, 0.0, 0.0, 0.0], 5)
        .build();

    let planner = QueryPlanner::new().with_vector_index_provider(&vector_index);

    let results = planner
        .execute(&query)
        .expect("Query planner execution must succeed");
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], doc_id);
}
