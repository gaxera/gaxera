use crate::object::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContiguousFrameObject {
    id: ObjectId,
    phys_base: u64,
    size_bytes: usize,
}

impl ContiguousFrameObject {
    pub fn new(id: ObjectId, phys_base: u64, size_bytes: usize) -> Self {
        Self {
            id,
            phys_base,
            size_bytes,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn phys_base(&self) -> u64 {
        self.phys_base
    }

    pub fn size_bytes(&self) -> usize {
        self.size_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_frame_object_metadata() {
        let frame = ContiguousFrameObject::new(ObjectId::new_for_test(1, 1), 0x10000000, 4096);
        assert_eq!(frame.phys_base(), 0x10000000);
        assert_eq!(frame.size_bytes(), 4096);
    }
}
