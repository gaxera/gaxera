use crate::object::ObjectId;
use alloc::vec::Vec;

/// A MemoryObject owns a collection of physical frames.
///
/// It shields userspace from physical fragmentation by allowing
/// disjoint physical frames to be mapped contiguously in an `AddressSpace`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryObject {
    id: ObjectId,
    frames: Vec<u64>,
    size_bytes: u64,
}

impl MemoryObject {
    pub fn new(id: ObjectId, size_bytes: u64) -> Self {
        Self {
            id,
            frames: Vec::new(),
            size_bytes,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn add_frame(&mut self, physical_address: u64) {
        self.frames.push(physical_address);
    }

    pub fn frames(&self) -> &[u64] {
        &self.frames
    }
}
