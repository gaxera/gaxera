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

    pub fn take_frames(&mut self) -> Vec<u64> {
        core::mem::take(&mut self.frames)
    }

    pub fn frames_subrange(
        &self,
        offset_bytes: u64,
        size_bytes: u64,
    ) -> Result<&[u64], &'static str> {
        if !offset_bytes.is_multiple_of(4096) || !size_bytes.is_multiple_of(4096) {
            return Err("Offset and size must be page aligned");
        }
        let end_bytes = offset_bytes
            .checked_add(size_bytes)
            .ok_or("Overflow in range")?;
        if end_bytes > self.size_bytes {
            return Err("Range exceeds memory object bounds");
        }
        let start_frame = (offset_bytes / 4096) as usize;
        let frame_count = (size_bytes / 4096) as usize;
        if start_frame + frame_count > self.frames.len() {
            return Err("Range exceeds allocated frame count");
        }
        Ok(&self.frames[start_frame..start_frame + frame_count])
    }
}
