//! Flash-Aligned Extent Allocator Module
//!
//! Allocates contiguous block extents aligned to flash erase-block boundaries.

use gaxfs_types::StorageError;

/// Extent Descriptor representing a contiguous range of storage blocks
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct ExtentDescriptor {
    pub start_block: u64,
    pub num_blocks: u32,
}

/// Bitmapped extent block allocator
#[derive(Debug)]
pub struct ExtentAllocator {
    total_blocks: u64,
    erase_block_align: u32,
    free_bitmap: alloc::vec::Vec<u64>,
}

impl ExtentAllocator {
    /// Creates a new extent allocator
    pub fn new(total_blocks: u64, erase_block_align: u32) -> Self {
        let bitmap_words = (total_blocks as usize).div_ceil(64);
        Self {
            total_blocks,
            erase_block_align,
            free_bitmap: alloc::vec![0u64; bitmap_words], // 0 = free, 1 = allocated
        }
    }

    /// Allocates contiguous block extent aligned to erase-block boundary
    pub fn allocate_extent(
        &mut self,
        requested_blocks: u32,
    ) -> Result<ExtentDescriptor, StorageError> {
        if requested_blocks == 0 {
            return Err(StorageError::OutOfBounds {
                requested: 0,
                max: self.total_blocks,
            });
        }

        let align = self.erase_block_align.max(1) as u64;
        let mut curr_block = 0u64;

        while curr_block + (requested_blocks as u64) <= self.total_blocks {
            // Align start block
            if !curr_block.is_multiple_of(align) {
                curr_block = ((curr_block / align) + 1) * align;
                continue;
            }

            // Check if contiguous range is free
            let mut is_free = true;
            for b in curr_block..(curr_block + requested_blocks as u64) {
                if self.is_block_allocated(b) {
                    is_free = false;
                    curr_block = b + 1;
                    break;
                }
            }

            if is_free {
                // Mark blocks as allocated
                for b in curr_block..(curr_block + requested_blocks as u64) {
                    self.set_block_allocated(b, true);
                }
                return Ok(ExtentDescriptor {
                    start_block: curr_block,
                    num_blocks: requested_blocks,
                });
            }
        }

        Err(StorageError::OutOfBounds {
            requested: requested_blocks as u64,
            max: self.total_blocks,
        })
    }

    /// Deallocates an extent range
    pub fn free_extent(&mut self, extent: ExtentDescriptor) {
        for b in extent.start_block..(extent.start_block + extent.num_blocks as u64) {
            if b < self.total_blocks {
                self.set_block_allocated(b, false);
            }
        }
    }

    fn is_block_allocated(&self, block: u64) -> bool {
        let word_idx = (block / 64) as usize;
        let bit_idx = (block % 64) as u8;
        (self.free_bitmap[word_idx] & (1u64 << bit_idx)) != 0
    }

    fn set_block_allocated(&mut self, block: u64, allocated: bool) {
        let word_idx = (block / 64) as usize;
        let bit_idx = (block % 64) as u8;
        if allocated {
            self.free_bitmap[word_idx] |= 1u64 << bit_idx;
        } else {
            self.free_bitmap[word_idx] &= !(1u64 << bit_idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extent_allocation_and_deallocation() {
        let mut allocator = ExtentAllocator::new(1024, 8); // Align to 8 blocks

        let extent1 = allocator.allocate_extent(16).unwrap();
        assert_eq!(
            extent1.start_block % 8,
            0,
            "Start block must be erase-block aligned"
        );
        assert_eq!(extent1.num_blocks, 16);

        let extent2 = allocator.allocate_extent(8).unwrap();
        assert_eq!(extent2.start_block, extent1.start_block + 16);

        allocator.free_extent(extent1);
        let extent3 = allocator.allocate_extent(16).unwrap();
        assert_eq!(
            extent3.start_block, extent1.start_block,
            "Should reuse freed aligned extent"
        );
    }
}
