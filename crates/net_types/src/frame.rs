//! Generic Transport-Independent Frame Descriptors.

use gaxera_abi::GaxObjectId;

/// Supported Frame Types.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u16)]
pub enum FrameType {
    Ethernet = 1,
    Loopback = 2,
    VirtualTransport = 3,
    EncryptedPayload = 4,
}

/// Bitflags for Frame Descriptors.
pub mod frame_flags {
    pub const START_OF_FRAME: u16 = 0x0001;
    pub const END_OF_FRAME: u16 = 0x0002;
    pub const ENCRYPTED: u16 = 0x0004;
    pub const CHECKSUM_OK: u16 = 0x0008;
}

/// Generic 32-byte Transport-Independent Frame Descriptor (`FrameDescriptor`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(C)]
pub struct FrameDescriptor {
    pub frame_type: u16,         // FrameType enum raw u16
    pub flags: u16,              // Bitflags
    pub payload_offset: u32,     // Shared memory offset in bytes
    pub payload_len: u32,        // Payload byte count
    pub session_id: GaxObjectId, // Associated NetSession UUID
    pub timestamp_ns: u64,       // Nanosecond timestamp
}

impl FrameDescriptor {
    pub const LEN: usize = 32;

    pub fn new(
        frame_type: FrameType,
        payload_offset: u32,
        payload_len: u32,
        session_id: GaxObjectId,
        timestamp_ns: u64,
    ) -> Self {
        Self {
            frame_type: frame_type as u16,
            flags: frame_flags::START_OF_FRAME | frame_flags::END_OF_FRAME,
            payload_offset,
            payload_len,
            session_id,
            timestamp_ns,
        }
    }
}
