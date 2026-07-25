//! 128-bit RFC 9562 UUIDv7 Object Identifier (`GaxObjectId`).

use core::fmt;

/// 128-bit RFC 9562 UUIDv7 Object Identifier.
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

    /// Return reference to raw 16 bytes.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }

    /// Constructs a UUIDv7 GaxObjectId from a 48-bit millisecond timestamp and random/sequence data.
    pub fn new_v7(timestamp_ms: u64, rand_high: u16, rand_low: u64) -> Self {
        let mut bytes = [0u8; 16];

        bytes[0] = ((timestamp_ms >> 40) & 0xFF) as u8;
        bytes[1] = ((timestamp_ms >> 32) & 0xFF) as u8;
        bytes[2] = ((timestamp_ms >> 24) & 0xFF) as u8;
        bytes[3] = ((timestamp_ms >> 16) & 0xFF) as u8;
        bytes[4] = ((timestamp_ms >> 8) & 0xFF) as u8;
        bytes[5] = (timestamp_ms & 0xFF) as u8;

        bytes[6] = 0x70 | (((rand_high >> 8) & 0x0F) as u8);
        bytes[7] = (rand_high & 0xFF) as u8;

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

    /// Generate a pseudo-random GaxObjectId for testing/runtime use.
    pub fn generate() -> Self {
        Self::new_v7(1700000000000, 0x1234, 0x56789ABCDEF01234)
    }
}

impl fmt::Debug for GaxObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
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
