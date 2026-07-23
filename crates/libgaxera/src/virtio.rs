extern crate alloc;

use core::sync::atomic::{fence, Ordering};

pub const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
pub const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
pub const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
pub const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;
pub const VIRTIO_PCI_CAP_PCI_CFG: u8 = 5;

pub const VRING_DESC_F_NEXT: u16 = 1;
pub const VRING_DESC_F_WRITE: u16 = 2;
pub const VRING_DESC_F_INDIRECT: u16 = 4;

pub const VIRTIO_F_VERSION_1: u64 = 1 << 32;

pub const VIRTIO_BLK_T_IN: u32 = 0;
pub const VIRTIO_BLK_T_OUT: u32 = 1;
pub const VIRTIO_BLK_T_FLUSH: u32 = 4;

pub const VIRTIO_BLK_S_OK: u8 = 0;
pub const VIRTIO_BLK_S_IOERR: u8 = 1;
pub const VIRTIO_BLK_S_UNSUPP: u8 = 2;

pub const VIRTIO_NET_F_MAC: u64 = 1 << 5;

/// VirtIO 1.0 Network Packet Header (12 bytes).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C, packed)]
pub struct VirtioNetHeader {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,
}

/// VirtIO 1.0 Block Request Header (16 bytes).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C, packed)]
pub struct VirtioBlkHeader {
    pub req_type: u32,
    pub reserved: u32,
    pub sector: u64,
}

/// Standardized VirtIO Transport Errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VirtioError {
    MissingCapability(u8),
    UnsupportedVersion,
    FeatureNegotiationFailed,
    MissingMandatoryFeature,
    InvalidQueueSize(u16),
    DmaAllocationFailed,
    QueueEnableFailed,
    DeviceResetFailed,
}

/// VirtIO 1.0 Descriptor Table Entry (16 bytes).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C, packed)]
pub struct VirtioDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

/// VirtIO 1.0 Available Ring Header.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C, packed)]
pub struct VirtioAvailHeader {
    pub flags: u16,
    pub idx: u16,
}

/// VirtIO 1.0 Used Ring Element.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(C, packed)]
pub struct VirtioUsedElem {
    pub id: u32,
    pub len: u32,
}

/// VirtIO 1.0 PCI Capability Structure Header parsed from config space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct VirtioPciCapability {
    pub cfg_type: u8,
    pub bar: u8,
    pub offset: u32,
    pub length: u32,
    pub notify_off_multiplier: u32,
}

/// VirtIO Virtqueue Abstraction with linear single-owner DMA memory.
pub struct Virtqueue {
    queue_index: u16,
    size: u16,
    notify_vaddr: u64,
    free_head: u16,
    num_free: u16,
    desc_base_vaddr: *mut VirtioDesc,
    avail_base_vaddr: *mut u8,
    used_base_vaddr: *const u8,
}

impl Virtqueue {
    /// Create a new Virtqueue wrapper over mapped DMA virtual memory.
    ///
    /// # Safety
    /// `vaddr` must point to a valid, zeroed, page-aligned DMA buffer for `size` descriptors.
    pub unsafe fn new(queue_index: u16, size: u16, notify_vaddr: u64, vaddr: *mut u8) -> Self {
        let desc_bytes = (size as usize) * core::mem::size_of::<VirtioDesc>();
        let desc_base_vaddr = vaddr as *mut VirtioDesc;
        let avail_base_vaddr = unsafe { vaddr.add(desc_bytes) };

        // Used ring is page-aligned (4096) after descriptor + avail ring
        let avail_bytes = 6 + (size as usize) * 2;
        let used_offset = (desc_bytes + avail_bytes + 4095) & !4095;
        let used_base_vaddr = unsafe { vaddr.add(used_offset) as *const u8 };

        // Chain free descriptor list
        for i in 0..(size - 1) {
            unsafe {
                let desc = desc_base_vaddr.add(i as usize);
                (*desc).next = i + 1;
            }
        }

        Self {
            queue_index,
            size,
            notify_vaddr,
            free_head: 0,
            num_free: size,
            desc_base_vaddr,
            avail_base_vaddr,
            used_base_vaddr,
        }
    }

    pub fn queue_index(&self) -> u16 {
        self.queue_index
    }

    pub fn size(&self) -> u16 {
        self.size
    }

    pub fn num_free(&self) -> u16 {
        self.num_free
    }

    pub fn notify_vaddr(&self) -> u64 {
        self.notify_vaddr
    }

    pub fn used_base_vaddr(&self) -> *const u8 {
        self.used_base_vaddr
    }

    /// Allocate a single descriptor slot from the free list.
    pub fn alloc_desc(&mut self) -> Option<u16> {
        if self.num_free == 0 {
            return None;
        }

        let head = self.free_head;
        unsafe {
            let desc = self.desc_base_vaddr.add(head as usize);
            self.free_head = (*desc).next;
        }

        self.num_free -= 1;
        Some(head)
    }

    /// Free a descriptor chain starting at `head`.
    pub fn free_chain(&mut self, mut head: u16) {
        let mut steps = 0;
        while head < self.size && steps < self.size && self.num_free < self.size {
            let next = unsafe {
                let desc = self.desc_base_vaddr.add(head as usize);
                let flags = (*desc).flags;
                let n = (*desc).next;
                (*desc).flags = 0;
                (*desc).next = self.free_head;
                if (flags & VRING_DESC_F_NEXT) != 0 {
                    n
                } else {
                    self.size
                }
            };
            self.free_head = head;
            self.num_free += 1;
            head = next;
            steps += 1;
        }
    }

    /// Push a descriptor chain head to the Available Ring with Release memory fence.
    pub fn submit_chain(&mut self, head: u16) {
        fence(Ordering::Release);
        unsafe {
            let idx_ptr = self.avail_base_vaddr.add(2) as *mut u16;
            let current_idx = idx_ptr.read_volatile();
            let ring_slot_ptr =
                self.avail_base_vaddr
                    .add(4 + ((current_idx % self.size) as usize) * 2) as *mut u16;
            ring_slot_ptr.write_volatile(head);
            fence(Ordering::Release);
            idx_ptr.write_volatile(current_idx.wrapping_add(1));
        }
    }

    /// Trigger doorbell write to notify_vaddr with SeqCst memory fence.
    ///
    /// # Safety
    /// `notify_vaddr` must point to valid mapped VirtIO Notify MMIO register.
    pub unsafe fn notify_doorbell(&self) {
        fence(Ordering::SeqCst);
        let ptr = self.notify_vaddr as *mut u16;
        unsafe {
            ptr.write_volatile(self.queue_index);
        }
    }
}

/// Computes VirtIO Modern notification MMIO virtual address.
pub fn calculate_notify_vaddr(
    notify_base_vaddr: u64,
    queue_notify_off: u16,
    notify_off_multiplier: u32,
) -> u64 {
    notify_base_vaddr + (queue_notify_off as u64) * (notify_off_multiplier as u64)
}

/// Negotiates Virtqueue size: min(requested, hardware_max), ensuring power of two.
pub fn negotiate_queue_size(requested: u16, hardware_max: u16) -> Result<u16, VirtioError> {
    if hardware_max == 0 {
        return Err(VirtioError::InvalidQueueSize(0));
    }
    let negotiated = requested.min(hardware_max);
    if negotiated == 0 || !negotiated.is_power_of_two() {
        return Err(VirtioError::InvalidQueueSize(negotiated));
    }
    Ok(negotiated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn virtqueue_descriptor_allocation_and_chaining() {
        let mut buffer = vec![0u8; 16384];
        let notify_vaddr = 0x5000_0000u64;
        let mut vq = unsafe { Virtqueue::new(0, 16, notify_vaddr, buffer.as_mut_ptr()) };

        assert_eq!(vq.num_free(), 16);
        assert_eq!(vq.notify_vaddr(), notify_vaddr);

        let desc0 = vq.alloc_desc().unwrap();
        let desc1 = vq.alloc_desc().unwrap();
        assert_eq!(desc0, 0);
        assert_eq!(desc1, 1);
        assert_eq!(vq.num_free(), 14);

        vq.free_chain(desc1);
        assert_eq!(vq.num_free(), 15);
    }

    #[test]
    fn test_calculate_notify_vaddr() {
        let base = 0x4000_0000u64;
        let off = 2u16;
        let mult = 4u32;
        let addr = calculate_notify_vaddr(base, off, mult);
        assert_eq!(addr, 0x4000_0008);
    }

    #[test]
    fn test_negotiate_queue_size() {
        assert_eq!(negotiate_queue_size(64, 256).unwrap(), 64);
        assert_eq!(negotiate_queue_size(512, 128).unwrap(), 128);
        assert!(negotiate_queue_size(0, 128).is_err());
        assert!(negotiate_queue_size(100, 128).is_err());
    }

    #[test]
    fn test_corrupted_chain_rejection() {
        let mut buffer = vec![0u8; 16384];
        let mut vq = unsafe { Virtqueue::new(0, 16, 0x5000_0000, buffer.as_mut_ptr()) };

        // Out-of-bounds head (>= 16) should be safely ignored without panic or loop
        vq.free_chain(99);
        assert_eq!(vq.num_free(), 16);
    }
}
