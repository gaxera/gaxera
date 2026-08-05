use crate::object::ObjectId;
use crate::resource::ResourceDomainId;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryObjectKind {
    Anonymous,
    ExecutableImage,
}

/// A MemoryObject owns a collection of physical frames.
///
/// Physical frames are released only when all three reference classes reach zero:
/// 1. capability_refs: active handles in any CapabilitySpace
/// 2. mapping_refs: active VMA mappings in any AddressSpace
/// 3. transient_refs: active in-flight kernel operation references
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryObject {
    id: ObjectId,
    charging_domain: ResourceDomainId,
    kind: MemoryObjectKind,
    frames: Vec<u64>,
    size_bytes: u64,
    capability_refs: u32,
    mapping_refs: u32,
    transient_refs: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryError {
    RefOverflow,
    RefUnderflow,
    InvalidRange,
}

impl MemoryObject {
    pub fn new(id: ObjectId, charging_domain: ResourceDomainId, size_bytes: u64) -> Self {
        Self::with_frames_and_kind(
            id,
            charging_domain,
            size_bytes,
            Vec::new(),
            MemoryObjectKind::Anonymous,
        )
    }

    pub fn new_image(id: ObjectId, charging_domain: ResourceDomainId, size_bytes: u64) -> Self {
        Self::with_frames_and_kind(
            id,
            charging_domain,
            size_bytes,
            Vec::new(),
            MemoryObjectKind::ExecutableImage,
        )
    }

    pub fn with_frames(
        id: ObjectId,
        charging_domain: ResourceDomainId,
        size_bytes: u64,
        frames: Vec<u64>,
    ) -> Self {
        Self::with_frames_and_kind(
            id,
            charging_domain,
            size_bytes,
            frames,
            MemoryObjectKind::Anonymous,
        )
    }

    pub fn with_frames_and_kind(
        id: ObjectId,
        charging_domain: ResourceDomainId,
        size_bytes: u64,
        frames: Vec<u64>,
        kind: MemoryObjectKind,
    ) -> Self {
        Self {
            id,
            charging_domain,
            kind,
            size_bytes,
            frames,
            capability_refs: 1,
            mapping_refs: 0,
            transient_refs: 0,
        }
    }

    pub fn kind(&self) -> MemoryObjectKind {
        self.kind
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn charging_domain(&self) -> ResourceDomainId {
        self.charging_domain
    }

    pub fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    pub fn add_frame(&mut self, physical_address: u64) -> Result<(), MemoryError> {
        self.frames
            .try_reserve(1)
            .map_err(|_| MemoryError::RefOverflow)?;
        self.frames.push(physical_address);
        Ok(())
    }

    pub fn frames(&self) -> &[u64] {
        &self.frames
    }

    pub fn take_frames(&mut self) -> Vec<u64> {
        core::mem::take(&mut self.frames)
    }

    pub fn capability_refs(&self) -> u32 {
        self.capability_refs
    }

    pub fn mapping_refs(&self) -> u32 {
        self.mapping_refs
    }

    pub fn transient_refs(&self) -> u32 {
        self.transient_refs
    }

    pub fn total_refs(&self) -> u32 {
        self.capability_refs + self.mapping_refs + self.transient_refs
    }

    pub fn can_destroy(&self) -> bool {
        self.total_refs() == 0
    }

    pub fn inc_capability_ref(&mut self) -> Result<(), MemoryError> {
        self.capability_refs = self
            .capability_refs
            .checked_add(1)
            .ok_or(MemoryError::RefOverflow)?;
        Ok(())
    }

    pub fn dec_capability_ref(&mut self) -> Result<bool, MemoryError> {
        self.capability_refs = self
            .capability_refs
            .checked_sub(1)
            .ok_or(MemoryError::RefUnderflow)?;
        Ok(self.can_destroy())
    }

    pub fn inc_mapping_ref(&mut self) -> Result<(), MemoryError> {
        self.mapping_refs = self
            .mapping_refs
            .checked_add(1)
            .ok_or(MemoryError::RefOverflow)?;
        Ok(())
    }

    pub fn dec_mapping_ref(&mut self) -> Result<bool, MemoryError> {
        self.mapping_refs = self
            .mapping_refs
            .checked_sub(1)
            .ok_or(MemoryError::RefUnderflow)?;
        Ok(self.can_destroy())
    }

    pub fn inc_transient_ref(&mut self) -> Result<(), MemoryError> {
        self.transient_refs = self
            .transient_refs
            .checked_add(1)
            .ok_or(MemoryError::RefOverflow)?;
        Ok(())
    }

    pub fn dec_transient_ref(&mut self) -> Result<bool, MemoryError> {
        self.transient_refs = self
            .transient_refs
            .checked_sub(1)
            .ok_or(MemoryError::RefUnderflow)?;
        Ok(self.can_destroy())
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

/// Record tracking an active virtual memory mapping (VMA) descendant of a MemoryObject.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappingLineageRecord {
    pub mapping_id: ObjectId,
    pub source_memory_object: ObjectId,
    pub target_address_space: ObjectId,
    pub virtual_address: u64,
    pub page_count: usize,
}

/// Registry mapping memory capabilities to active virtual address space mappings across processes.
#[derive(Debug, Default)]
pub struct MappingLineageTable {
    records: Vec<MappingLineageRecord>,
}

impl MappingLineageTable {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub fn add_mapping(&mut self, record: MappingLineageRecord) {
        self.records.push(record);
    }

    pub fn remove_mapping(&mut self, mapping_id: ObjectId) -> Option<MappingLineageRecord> {
        if let Some(pos) = self.records.iter().position(|r| r.mapping_id == mapping_id) {
            Some(self.records.remove(pos))
        } else {
            None
        }
    }

    pub fn find_mappings_by_memory_object(
        &self,
        source_memory_object: ObjectId,
    ) -> Vec<MappingLineageRecord> {
        self.records
            .iter()
            .filter(|r| r.source_memory_object == source_memory_object)
            .cloned()
            .collect()
    }

    pub fn remove_mappings_by_memory_object(
        &mut self,
        source_memory_object: ObjectId,
    ) -> Vec<MappingLineageRecord> {
        let mut matching = Vec::new();
        let mut i = 0;
        while i < self.records.len() {
            if self.records[i].source_memory_object == source_memory_object {
                matching.push(self.records.remove(i));
            } else {
                i += 1;
            }
        }
        matching
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource::{ResourceDomain, ResourceLimits};

    fn test_obj_id(raw: u32) -> ObjectId {
        ObjectId::new_for_test(raw, 1)
    }

    fn test_domain(id: u32) -> ResourceDomain {
        ResourceDomain::new_for_test(
            ResourceDomainId::new_for_test(id),
            ResourceLimits {
                objects: 16,
                capabilities: 16,
                memory_bytes: 65536,
            },
        )
    }

    #[test]
    fn case_a_ordinary_process_exit_reclamation() {
        let creator_domain = test_domain(1);
        let obj_id = test_obj_id(10);
        let mut mem_obj = MemoryObject::new(obj_id, creator_domain.id(), 4096);

        // Created with 1 capability ref
        assert_eq!(mem_obj.capability_refs(), 1);
        assert_eq!(mem_obj.mapping_refs(), 0);
        assert!(!mem_obj.can_destroy());

        // Process maps memory -> inc mapping ref
        mem_obj.inc_mapping_ref().unwrap();
        assert_eq!(mem_obj.total_refs(), 2);

        // Process exits: capability handle dropped, VMA unmapped
        assert_eq!(mem_obj.dec_capability_ref(), Ok(false));
        assert_eq!(mem_obj.dec_mapping_ref(), Ok(true)); // Returns true: total_refs == 0

        assert!(mem_obj.can_destroy());
    }

    #[test]
    fn case_b_delegated_memory_survival_after_creator_exit() {
        let creator_domain = test_domain(1);
        let obj_id = test_obj_id(20);
        let mut mem_obj = MemoryObject::new(obj_id, creator_domain.id(), 4096);

        // Creator delegates handle to Process B -> inc capability ref
        mem_obj.inc_capability_ref().unwrap(); // cap_refs = 2
        // Process B maps memory -> inc mapping ref
        mem_obj.inc_mapping_ref().unwrap(); // map_refs = 1

        assert_eq!(mem_obj.total_refs(), 3);

        // Creator process exits: Creator handle dropped, Creator VMA unmapped
        assert_eq!(mem_obj.dec_capability_ref(), Ok(false)); // cap_refs = 1
        assert!(!mem_obj.can_destroy());

        // Memory object remains valid and accessible to Process B
        assert_eq!(mem_obj.capability_refs(), 1);
        assert_eq!(mem_obj.mapping_refs(), 1);
        assert_eq!(mem_obj.total_refs(), 2);

        // Process B drops handle and unmaps VMA (Case D final release)
        assert_eq!(mem_obj.dec_capability_ref(), Ok(false)); // cap_refs = 0
        assert_eq!(mem_obj.dec_mapping_ref(), Ok(true)); // map_refs = 0 -> total_refs == 0
        assert!(mem_obj.can_destroy());
    }

    #[test]
    fn case_c_supervisor_revocation_lineage_cascade() {
        let creator_domain = test_domain(1);
        let obj_id = test_obj_id(30);
        let mut mem_obj = MemoryObject::new(obj_id, creator_domain.id(), 4096);

        let mut lineage = MappingLineageTable::new();
        lineage.add_mapping(MappingLineageRecord {
            mapping_id: test_obj_id(100),
            source_memory_object: obj_id,
            target_address_space: test_obj_id(200),
            virtual_address: 0x0000_6000_0000_0000,
            page_count: 1,
        });

        mem_obj.inc_capability_ref().unwrap(); // Delegated handle
        mem_obj.inc_mapping_ref().unwrap(); // Process B mapping
        assert_eq!(mem_obj.total_refs(), 3);

        // Supervisor calls Revoke on root capability:
        // 1. Walk lineage table to unmap VMAs
        let revoked_mappings = lineage.remove_mappings_by_memory_object(obj_id);
        assert_eq!(revoked_mappings.len(), 1);

        // 2. Decrement mapping and capability references individually
        mem_obj.dec_mapping_ref().unwrap();
        mem_obj.dec_capability_ref().unwrap();
        assert_eq!(mem_obj.dec_capability_ref(), Ok(true));
        assert_eq!(mem_obj.total_refs(), 0);
        assert!(mem_obj.can_destroy());
    }
}
