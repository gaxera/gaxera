#![allow(dead_code)]
use crate::syscall;
use core::alloc::Layout;
use gaxera_abi::{Handle, Rights};

const CHUNK_SIZE: u64 = 64 * 1024; // 64 KiB
const MAX_BLOCKS: usize = 1024;
const HEAP_START_VADDR: u64 = 0x0000_6000_0000_0000;
const HEAP_END_VADDR: u64 = 0x0000_7C00_0000_0000;

#[derive(Copy, Clone)]
struct BlockMeta {
    active: bool,
    vaddr: u64,
    size: usize,
    is_free: bool,
}

impl BlockMeta {
    const EMPTY: Self = Self {
        active: false,
        vaddr: 0,
        size: 0,
        is_free: false,
    };
}

pub struct HeapArena {
    blocks: [BlockMeta; MAX_BLOCKS],
    chunk_handles: [(Handle, Handle); 64], // Tracks (mem_obj, mapping) for teardown
    chunk_count: usize,
    factory_handle: Handle,
    aspace_handle: Handle,
    next_vaddr: u64,
    pub initialized: bool,
}

impl Default for HeapArena {
    fn default() -> Self {
        Self::new()
    }
}

impl HeapArena {
    pub const fn new() -> Self {
        Self {
            blocks: [BlockMeta::EMPTY; MAX_BLOCKS],
            chunk_handles: [(Handle::INVALID, Handle::INVALID); 64],
            chunk_count: 0,
            factory_handle: Handle::INVALID,
            aspace_handle: Handle::INVALID,
            next_vaddr: HEAP_START_VADDR,
            initialized: false,
        }
    }

    pub fn init(&mut self, factory: Handle, aspace: Handle) {
        self.factory_handle = factory;
        self.aspace_handle = aspace;
        self.initialized = true;
    }

    pub fn teardown(&mut self) {
        // Manually delete all capability handles to trigger physical frame reclamation.
        for i in 0..self.chunk_count {
            let (mem_obj, mapping) = self.chunk_handles[i];
            let _ = syscall::delete_handle(mapping);
            let _ = syscall::delete_handle(mem_obj);
            self.chunk_handles[i] = (Handle::INVALID, Handle::INVALID);
        }
        self.chunk_count = 0;
        self.blocks = [BlockMeta::EMPTY; MAX_BLOCKS];
        self.factory_handle = Handle::INVALID;
        self.aspace_handle = Handle::INVALID;
        self.next_vaddr = HEAP_START_VADDR;
        self.initialized = false;
    }

    pub fn allocate(&mut self, layout: Layout) -> *mut u8 {
        if !self.initialized {
            return core::ptr::null_mut();
        }

        let size = core::cmp::max(layout.size(), layout.align());
        let align = layout.align();

        // 1. Try to find a free block that fits
        if let Some(idx) = self.find_free_block(size, align) {
            return self.split_and_allocate(idx, size, align);
        }

        // Coalescing is deliberately deferred until a fitting block cannot be
        // found. Deallocation is a hot path for fragmented workloads, and
        // eagerly scanning the entire metadata table there turns repeated
        // alloc/free cycles into an avoidable quadratic cost.
        self.coalesce();
        if let Some(idx) = self.find_free_block(size, align) {
            return self.split_and_allocate(idx, size, align);
        }

        // 2. Grow until the request fits or the bounded arena is exhausted.
        // A single allocation may legitimately span multiple fixed-size chunks.
        loop {
            if !self.grow_heap() {
                return core::ptr::null_mut(); // OOM
            }
            if let Some(idx) = self.find_free_block(size, align) {
                return self.split_and_allocate(idx, size, align);
            }
        }
    }

    pub fn deallocate(&mut self, ptr: *mut u8, _layout: Layout) {
        let vaddr = ptr as u64;
        for i in 0..MAX_BLOCKS {
            if self.blocks[i].active && !self.blocks[i].is_free && self.blocks[i].vaddr == vaddr {
                self.blocks[i].is_free = true;
                return;
            }
        }
    }

    fn find_free_block(&self, size: usize, align: usize) -> Option<usize> {
        self.blocks.iter().position(|b| {
            if b.active && b.is_free && b.size >= size {
                let align_offset = b.vaddr % (align as u64);
                let adjustment = if align_offset == 0 {
                    0
                } else {
                    (align as u64) - align_offset
                };
                size.checked_add(adjustment as usize)
                    .is_some_and(|required| b.size >= required)
            } else {
                false
            }
        })
    }

    fn split_and_allocate(&mut self, idx: usize, size: usize, align: usize) -> *mut u8 {
        let b = self.blocks[idx];
        let align_offset = b.vaddr % (align as u64);
        let adjustment = if align_offset == 0 {
            0
        } else {
            (align as u64) - align_offset
        };

        let alloc_vaddr = b.vaddr + adjustment;
        let remaining_size = b.size - size - (adjustment as usize);

        self.blocks[idx].is_free = false;
        self.blocks[idx].vaddr = alloc_vaddr;
        self.blocks[idx].size = size;

        // If there's space before the aligned start, create a free block
        let opt_free_idx_before = self.find_inactive_slot();
        if adjustment > 0 {
            if let Some(free_idx) = opt_free_idx_before {
                self.blocks[free_idx] = BlockMeta {
                    active: true,
                    vaddr: b.vaddr,
                    size: adjustment as usize,
                    is_free: true,
                };
            }
        }

        // If there's space after the allocation, create a free block
        let opt_free_idx_after = self.find_inactive_slot();
        if remaining_size > 0 {
            if let Some(free_idx) = opt_free_idx_after {
                self.blocks[free_idx] = BlockMeta {
                    active: true,
                    vaddr: alloc_vaddr + (size as u64),
                    size: remaining_size,
                    is_free: true,
                };
            }
        }

        alloc_vaddr as *mut u8
    }

    fn find_inactive_slot(&self) -> Option<usize> {
        self.blocks.iter().position(|b| !b.active)
    }

    fn coalesce(&mut self) {
        // Simple O(N^2) coalescing for adjacent free blocks
        let mut changed = true;
        while changed {
            changed = false;
            for i in 0..MAX_BLOCKS {
                if !self.blocks[i].active || !self.blocks[i].is_free {
                    continue;
                }
                let Some(end_vaddr) = self.blocks[i].vaddr.checked_add(self.blocks[i].size as u64)
                else {
                    continue;
                };

                for j in 0..MAX_BLOCKS {
                    if i != j
                        && self.blocks[j].active
                        && self.blocks[j].is_free
                        && self.blocks[j].vaddr == end_vaddr
                    {
                        // Coalesce j into i
                        self.blocks[i].size += self.blocks[j].size;
                        self.blocks[j].active = false;
                        changed = true;
                    }
                }
            }
        }
    }

    fn grow_heap(&mut self) -> bool {
        // Every mapped chunk must remain represented so teardown can release
        // both capabilities. Refuse growth before issuing syscalls when the
        // bounded metadata table is full.
        if self.chunk_count >= self.chunk_handles.len()
            || self.next_vaddr >= HEAP_END_VADDR
            || HEAP_END_VADDR - self.next_vaddr < CHUNK_SIZE
        {
            return false;
        }
        let next_vaddr = match self.next_vaddr.checked_add(CHUNK_SIZE) {
            Some(next) => next,
            None => return false,
        };

        // 1. Request capability
        let mem_obj_res = syscall::factory_create_memory_object(self.factory_handle, CHUNK_SIZE);
        let mem_obj = match mem_obj_res {
            Ok(h) => h,
            Err(_) => return false,
        };

        // 2. Map capability
        let map_res = syscall::map_memory(
            self.aspace_handle,
            mem_obj,
            self.next_vaddr,
            Rights::READ | Rights::WRITE | Rights::MAP,
        );

        // 3. Compensating transaction on mapping failure
        let _mapping_handle = match map_res {
            Ok(h) => h,
            Err(_) => {
                let _ = syscall::delete_handle(mem_obj); // Rollback memory & quota
                return false;
            }
        };

        // We mapped it successfully, add to free blocks
        if let Some(idx) = self.find_inactive_slot() {
            self.chunk_handles[self.chunk_count] = (mem_obj, _mapping_handle);
            self.chunk_count += 1;

            self.blocks[idx] = BlockMeta {
                active: true,
                vaddr: self.next_vaddr,
                size: CHUNK_SIZE as usize,
                is_free: true,
            };
            self.next_vaddr = next_vaddr;
            self.coalesce();
            true
        } else {
            // Unrecoverable state: mapped memory but out of block metadata
            let _ = syscall::delete_handle(_mapping_handle);
            let _ = syscall::delete_handle(mem_obj);
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::alloc::Layout;

    #[test]
    fn test_allocator_coalesce() {
        let mut arena = HeapArena::new();
        // Since we can't test actual syscalls in unit test without mocking,
        // we manually inject a block
        arena.blocks[0] = BlockMeta {
            active: true,
            vaddr: 0x1000,
            size: 1024,
            is_free: true,
        };
        arena.blocks[1] = BlockMeta {
            active: true,
            vaddr: 0x1000 + 1024,
            size: 1024,
            is_free: true,
        };
        arena.coalesce();

        let mut free_count = 0;
        let mut total_size = 0;
        for b in &arena.blocks {
            if b.active {
                free_count += 1;
                total_size += b.size;
            }
        }
        assert_eq!(free_count, 1);
        assert_eq!(total_size, 2048);
    }

    #[test]
    fn test_allocator_oom_uninitialized() {
        let mut arena = HeapArena::new();
        let layout = Layout::from_size_align(32, 8).unwrap();
        let ptr = arena.allocate(layout);
        assert!(ptr.is_null()); // Should return null when uninitialized
    }

    #[test]
    fn test_allocator_alignment() {
        let mut arena = HeapArena::new();
        // Initialize manually to bypass syscalls
        arena.initialized = true;

        // Inject a 64KiB block at a weird unaligned address
        arena.blocks[0] = BlockMeta {
            active: true,
            vaddr: 0x1003, // Not aligned to 8 or 16
            size: CHUNK_SIZE as usize,
            is_free: true,
        };

        let layout = Layout::from_size_align(32, 16).unwrap();
        let ptr = arena.allocate(layout);

        // Should not be null and should be 16-byte aligned
        assert!(!ptr.is_null());
        assert_eq!((ptr as u64) % 16, 0);

        // Let's also verify that we can deallocate and coalesce
        arena.deallocate(ptr, layout);
        arena.coalesce();
    }
}
