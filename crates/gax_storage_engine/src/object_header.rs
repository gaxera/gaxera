//! GaxFS Object Header & Relationship Serialization Module
//!
//! Encodes payload extent pointers, key-value metadata attributes,
//! and authoritative directed graph relationship links directly in object headers.

use crate::extent_allocator::ExtentDescriptor;
use crate::integrity::{compute_checksum, verify_checksum};
use alloc::string::String;
use alloc::vec::Vec;
use gaxfs_types::{GaxObjectId, StorageError};

/// First-Class Directed Relationship Type
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[repr(u16)]
pub enum RelationshipKind {
    GeneratedFrom = 1,
    DependsOn = 2,
    References = 3,
    Contains = 4,
    BelongsTo = 5,
}

/// Directed Relationship Edge stored in Object Header
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct RelationshipEdge {
    pub kind: RelationshipKind,
    pub target_id: GaxObjectId,
}

/// Authoritative On-Disk Object Header
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GaxFsObjectHeader {
    pub object_id: GaxObjectId,
    pub payload_size: u64,
    pub extents: Vec<ExtentDescriptor>,
    pub attributes: Vec<(String, String)>,
    pub relationships: Vec<RelationshipEdge>,
    pub checksum: [u8; 32],
}

impl GaxFsObjectHeader {
    pub fn new(object_id: GaxObjectId, payload_size: u64) -> Self {
        let mut header = Self {
            object_id,
            payload_size,
            extents: Vec::new(),
            attributes: Vec::new(),
            relationships: Vec::new(),
            checksum: [0u8; 32],
        };
        header.update_checksum();
        header
    }

    /// Serializes object header into byte buffer
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        buf.extend_from_slice(self.object_id.as_bytes());
        buf.extend_from_slice(&self.payload_size.to_le_bytes());

        // Extents
        buf.extend_from_slice(&(self.extents.len() as u32).to_le_bytes());
        for ext in &self.extents {
            buf.extend_from_slice(&ext.start_block.to_le_bytes());
            buf.extend_from_slice(&ext.num_blocks.to_le_bytes());
        }

        // Attributes
        buf.extend_from_slice(&(self.attributes.len() as u32).to_le_bytes());
        for (k, v) in &self.attributes {
            buf.extend_from_slice(&(k.len() as u16).to_le_bytes());
            buf.extend_from_slice(k.as_bytes());
            buf.extend_from_slice(&(v.len() as u16).to_le_bytes());
            buf.extend_from_slice(v.as_bytes());
        }

        // Relationships
        buf.extend_from_slice(&(self.relationships.len() as u32).to_le_bytes());
        for rel in &self.relationships {
            buf.extend_from_slice(&(rel.kind as u16).to_le_bytes());
            buf.extend_from_slice(rel.target_id.as_bytes());
        }

        let checksum = compute_checksum(&buf);
        buf.extend_from_slice(&checksum);

        buf
    }

    /// Deserializes object header from byte buffer
    pub fn deserialize(buf: &[u8]) -> Result<Self, StorageError> {
        if buf.len() < 32 + 28 {
            return Err(StorageError::OutOfBounds {
                requested: buf.len() as u64,
                max: 60,
            });
        }

        let data_len = buf.len() - 32;
        let body = &buf[..data_len];
        let expected_checksum: &[u8; 32] = buf[data_len..].try_into().unwrap();

        if !verify_checksum(body, expected_checksum) {
            return Err(StorageError::ChecksumMismatch);
        }

        let mut offset = 0;

        let mut obj_bytes = [0u8; 16];
        obj_bytes.copy_from_slice(&body[offset..offset + 16]);
        let object_id = GaxObjectId::from_bytes(obj_bytes);
        offset += 16;

        let payload_size = u64::from_le_bytes(body[offset..offset + 8].try_into().unwrap());
        offset += 8;

        // Extents
        let extent_count =
            u32::from_le_bytes(body[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let mut extents = Vec::with_capacity(extent_count);
        for _ in 0..extent_count {
            let start_block = u64::from_le_bytes(body[offset..offset + 8].try_into().unwrap());
            offset += 8;
            let num_blocks = u32::from_le_bytes(body[offset..offset + 4].try_into().unwrap());
            offset += 4;
            extents.push(ExtentDescriptor {
                start_block,
                num_blocks,
            });
        }

        // Attributes
        let attr_count = u32::from_le_bytes(body[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let mut attributes = Vec::with_capacity(attr_count);
        for _ in 0..attr_count {
            let k_len = u16::from_le_bytes(body[offset..offset + 2].try_into().unwrap()) as usize;
            offset += 2;
            let k = String::from_utf8(body[offset..offset + k_len].to_vec())
                .map_err(|_| StorageError::ChecksumMismatch)?;
            offset += k_len;

            let v_len = u16::from_le_bytes(body[offset..offset + 2].try_into().unwrap()) as usize;
            offset += 2;
            let v = String::from_utf8(body[offset..offset + v_len].to_vec())
                .map_err(|_| StorageError::ChecksumMismatch)?;
            offset += v_len;

            attributes.push((k, v));
        }

        // Relationships
        let rel_count = u32::from_le_bytes(body[offset..offset + 4].try_into().unwrap()) as usize;
        offset += 4;
        let mut relationships = Vec::with_capacity(rel_count);
        for _ in 0..rel_count {
            let kind_raw = u16::from_le_bytes(body[offset..offset + 2].try_into().unwrap());
            offset += 2;
            let kind = match kind_raw {
                1 => RelationshipKind::GeneratedFrom,
                2 => RelationshipKind::DependsOn,
                3 => RelationshipKind::References,
                4 => RelationshipKind::Contains,
                5 => RelationshipKind::BelongsTo,
                _ => return Err(StorageError::ChecksumMismatch),
            };

            let mut rel_obj_bytes = [0u8; 16];
            rel_obj_bytes.copy_from_slice(&body[offset..offset + 16]);
            offset += 16;

            relationships.push(RelationshipEdge {
                kind,
                target_id: GaxObjectId::from_bytes(rel_obj_bytes),
            });
        }

        Ok(Self {
            object_id,
            payload_size,
            extents,
            attributes,
            relationships,
            checksum: *expected_checksum,
        })
    }

    pub fn update_checksum(&mut self) {
        let serialized = self.serialize();
        let data_len = serialized.len() - 32;
        self.checksum.copy_from_slice(&serialized[data_len..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_header_serialization_with_relationships() {
        let obj_id = GaxObjectId::new_v7(1000, 1, 2);
        let target_id = GaxObjectId::new_v7(2000, 3, 4);

        let mut header = GaxFsObjectHeader::new(obj_id, 4096);
        header.extents.push(ExtentDescriptor {
            start_block: 64,
            num_blocks: 8,
        });
        header.attributes.push(("author".into(), "nyx".into()));
        header.relationships.push(RelationshipEdge {
            kind: RelationshipKind::GeneratedFrom,
            target_id,
        });

        let buf = header.serialize();
        let recovered = GaxFsObjectHeader::deserialize(&buf)
            .expect("Object header deserialization must succeed");

        assert_eq!(recovered.object_id, obj_id);
        assert_eq!(recovered.payload_size, 4096);
        assert_eq!(recovered.extents.len(), 1);
        assert_eq!(recovered.attributes.len(), 1);
        assert_eq!(recovered.attributes[0], ("author".into(), "nyx".into()));
        assert_eq!(recovered.relationships.len(), 1);
        assert_eq!(
            recovered.relationships[0].kind,
            RelationshipKind::GeneratedFrom
        );
        assert_eq!(recovered.relationships[0].target_id, target_id);
    }
}
