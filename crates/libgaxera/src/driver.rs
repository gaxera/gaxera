use gaxera_abi::Handle;

/// Owned handle to a physically contiguous DMA frame object.
#[derive(Debug)]
pub struct ContiguousFrameHandle {
    handle: Handle,
    phys_base: u64,
    size_bytes: usize,
}

impl ContiguousFrameHandle {
    pub const fn from_parts(handle: Handle, phys_base: u64, size_bytes: usize) -> Self {
        Self {
            handle,
            phys_base,
            size_bytes,
        }
    }

    pub const fn handle(&self) -> Handle {
        self.handle
    }

    pub const fn phys_base(&self) -> u64 {
        self.phys_base
    }

    pub const fn size_bytes(&self) -> usize {
        self.size_bytes
    }
}

/// Helper wrapper for device-scoped PCI ECAM configuration register manipulation.
#[derive(Debug)]
pub struct PciDeviceConfig {
    ecam_page_vaddr: u64,
}

impl PciDeviceConfig {
    pub const fn new(ecam_page_vaddr: u64) -> Self {
        Self { ecam_page_vaddr }
    }

    pub fn vaddr(&self) -> u64 {
        self.ecam_page_vaddr
    }

    /// Read 16-bit register from device configuration space.
    ///
    /// # Safety
    /// `offset` must be 2-byte aligned and within the 4KB device ECAM page window.
    pub unsafe fn read_u16(&self, offset: usize) -> u16 {
        debug_assert!(offset + 2 <= 4096);
        let ptr = (self.ecam_page_vaddr + offset as u64) as *const u16;
        // SAFETY: Direct volatile read from mapped 4KB device ECAM page.
        unsafe { ptr.read_volatile() }
    }

    /// Write 16-bit register in device configuration space.
    ///
    /// # Safety
    /// `offset` must be 2-byte aligned and within the 4KB device ECAM page window.
    pub unsafe fn write_u16(&self, offset: usize, val: u16) {
        debug_assert!(offset + 2 <= 4096);
        let ptr = (self.ecam_page_vaddr + offset as u64) as *mut u16;
        // SAFETY: Direct volatile write to mapped 4KB device ECAM page.
        unsafe { ptr.write_volatile(val) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contiguous_frame_handle_properties() {
        let handle = Handle::from_parts(5, 1);
        let dma = ContiguousFrameHandle::from_parts(handle, 0x20000000, 8192);
        assert_eq!(dma.phys_base(), 0x20000000);
        assert_eq!(dma.size_bytes(), 8192);
    }
}
