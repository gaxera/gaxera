//! GaxObjectId Representation (128-bit RFC 9562 UUIDv7)
//!
//! Immutable, location-independent unique identifier for GaxFS objects.

use core::fmt;

/// 128-bit RFC 9562 UUIDv7 GaxFS Object Identifier.
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct GaxObjectId {
    bytes: [u8; 16],
}

impl GaxObjectId {
    /// Nil (all zero) GaxObjectId
    pub const NIL: Self = Self { bytes: [0u8; 16] };

    /// Creates a GaxObjectId from raw 16-byte array
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }

    /// Constructs a UUIDv7 GaxObjectId from a 48-bit millisecond timestamp and 74 bits of random/sequence data.
    pub fn new_v7(timestamp_ms: u64, rand_high: u16, rand_low: u64) -> Self {
        let mut bytes = [0u8; 16];

        // 48-bit timestamp in big-endian
        bytes[0] = ((timestamp_ms >> 40) & 0xFF) as u8;
        bytes[1] = ((timestamp_ms >> 32) & 0xFF) as u8;
        bytes[2] = ((timestamp_ms >> 24) & 0xFF) as u8;
        bytes[3] = ((timestamp_ms >> 16) & 0xFF) as u8;
        bytes[4] = ((timestamp_ms >> 8) & 0xFF) as u8;
        bytes[5] = (timestamp_ms & 0xFF) as u8;

        // Version 7 (0b0111) in top 4 bits of byte 6
        bytes[6] = 0x70 | (((rand_high >> 8) & 0x0F) as u8);
        bytes[7] = (rand_high & 0xFF) as u8;

        // Variant 10xx in top 2 bits of byte 8
        bytes[8] = 0x80 | (((rand_low >> 56) & 0x3F) as u8);
        bytes[9] = ((rand_low >> 48) & 0xFF) as u8;
        bytes[10] = ((rand_low >> 40) & 0xFF) as u8;
        bytes[11] = ((rand_low >> 32) & 0xFF) as u8;
        bytes[12] = ((rand_low >> 24) & 0xFF) as u8;
        bytes[13] = ((rand_low >> 16) & 0xFF) as u8;
        bytes[14] = ((rand_low >> 8) & 0xFF) as u8;
        bytes[15] = (rand_low & 0xFF) as u8;

        Self { bytes }
    }

    /// Returns reference to raw bytes
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }

    /// Consumes object and returns raw bytes
    pub const fn to_bytes(self) -> [u8; 16] {
        self.bytes
    }

    /// Returns true if this is a Nil ID
    pub const fn is_nil(&self) -> bool {
        let mut i = 0;
        while i < 16 {
            if self.bytes[i] != 0 {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Extracts timestamp component in milliseconds
    pub fn timestamp_ms(&self) -> u64 {
        ((self.bytes[0] as u64) << 40)
            | ((self.bytes[1] as u64) << 32)
            | ((self.bytes[2] as u64) << 24)
            | ((self.bytes[3] as u64) << 16)
            | ((self.bytes[4] as u64) << 8)
            | (self.bytes[5] as u64)
    }
}

impl fmt::Debug for GaxObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "GaxObjectId({:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x})",
            self.bytes[0],
            self.bytes[1],
            self.bytes[2],
            self.bytes[3],
            self.bytes[4],
            self.bytes[5],
            self.bytes[6],
            self.bytes[7],
            self.bytes[8],
            self.bytes[9],
            self.bytes[10],
            self.bytes[11],
            self.bytes[12],
            self.bytes[13],
            self.bytes[14],
            self.bytes[15]
        )
    }
}

impl fmt::Display for GaxObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuidv7_construction_and_ordering() {
        let id1 = GaxObjectId::new_v7(1000, 0x123, 0x456);
        let id2 = GaxObjectId::new_v7(2000, 0x123, 0x456);

        assert_eq!(id1.timestamp_ms(), 1000);
        assert_eq!(id2.timestamp_ms(), 2000);
        assert!(id1 < id2, "ID with earlier timestamp must sort lower");
    }

    #[test]
    fn test_nil_id() {
        assert!(GaxObjectId::NIL.is_nil());
        let id = GaxObjectId::new_v7(1, 0, 0);
        assert!(!id.is_nil());
    }
}
