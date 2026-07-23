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

/// Validates block request bounds, DMA alignment, and capacity limits.
pub fn validate_block_request(
    sector: u64,
    sector_count: u64,
    total_capacity_sectors: u64,
    dma: &ContiguousFrameHandle,
) -> Result<(), BlockError> {
    if sector_count == 0 || dma.phys_base() & 0xFFF != 0 {
        return Err(BlockError::InvalidParameter);
    }
    let end_sector = sector
        .checked_add(sector_count)
        .ok_or(BlockError::InvalidParameter)?;
    if end_sector > total_capacity_sectors {
        return Err(BlockError::InvalidParameter);
    }
    Ok(())
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

    #[test]
    fn test_validate_block_request() {
        let handle = Handle::from_parts(1, 1);
        let valid_dma = ContiguousFrameHandle::from_parts(handle, 0x1000_0000, 4096);
        let misaligned_dma = ContiguousFrameHandle::from_parts(handle, 0x1000_0100, 4096);

        // Valid request
        assert!(validate_block_request(0, 8, 2048, &valid_dma).is_ok());

        // Out-of-bounds request
        assert_eq!(
            validate_block_request(2040, 16, 2048, &valid_dma),
            Err(BlockError::InvalidParameter)
        );

        // Misaligned DMA handle
        assert_eq!(
            validate_block_request(0, 8, 2048, &misaligned_dma),
            Err(BlockError::InvalidParameter)
        );
    }
}
