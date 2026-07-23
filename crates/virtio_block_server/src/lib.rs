#![no_std]

extern crate alloc;

use gaxera_abi::block::BlockError;
use gaxera_abi::Handle;
use libgaxera::driver::ContiguousFrameHandle;
use libgaxera::virtio::{
    VirtioBlkHeader, VIRTIO_BLK_S_IOERR, VIRTIO_BLK_S_OK, VIRTIO_BLK_S_UNSUPP,
};

/// Owned Block Request holding payload and completion state.
pub struct BlockRequest {
    pub header: VirtioBlkHeader,
    pub dma_handle: ContiguousFrameHandle,
    pub status_byte: u8,
    pub caller_handle: Option<Handle>,
}

impl BlockRequest {
    pub fn new(
        req_type: u32,
        sector: u64,
        dma_handle: ContiguousFrameHandle,
        caller_handle: Option<Handle>,
    ) -> Self {
        Self {
            header: VirtioBlkHeader {
                req_type,
                reserved: 0,
                sector,
            },
            dma_handle,
            status_byte: 0xFF, // Initial sentinel value
            caller_handle,
        }
    }
}

/// Translates raw VirtIO status byte to standardized `BlockError`.
pub fn translate_virtio_status(status_byte: u8) -> BlockError {
    match status_byte {
        VIRTIO_BLK_S_OK => BlockError::Success,
        VIRTIO_BLK_S_IOERR => BlockError::IoError,
        VIRTIO_BLK_S_UNSUPP => BlockError::UnsupportedOperation,
        _ => BlockError::DeviceFailure,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_virtio_status() {
        assert_eq!(
            translate_virtio_status(VIRTIO_BLK_S_OK),
            BlockError::Success
        );
        assert_eq!(
            translate_virtio_status(VIRTIO_BLK_S_IOERR),
            BlockError::IoError
        );
        assert_eq!(
            translate_virtio_status(VIRTIO_BLK_S_UNSUPP),
            BlockError::UnsupportedOperation
        );
        assert_eq!(translate_virtio_status(0xFF), BlockError::DeviceFailure);
    }
}
