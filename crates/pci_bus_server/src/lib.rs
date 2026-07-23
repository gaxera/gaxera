#![no_std]

extern crate alloc;

use alloc::vec::Vec;
use gaxera_abi::pci::{PciCapabilityEntry, PciDeviceHeader, PciSegmentGroup};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredPciDevice {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub header: PciDeviceHeader,
    pub capabilities: Vec<PciCapabilityEntry>,
}

pub struct PciBusServer {
    devices: Vec<DiscoveredPciDevice>,
}

impl Default for PciBusServer {
    fn default() -> Self {
        Self::new()
    }
}

impl PciBusServer {
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
        }
    }

    pub fn devices(&self) -> &[DiscoveredPciDevice] {
        &self.devices
    }

    /// Lookup a discovered PCI device by vendor and device ID.
    pub fn find_device(&self, vendor_id: u16, device_id: u16) -> Option<&DiscoveredPciDevice> {
        self.devices
            .iter()
            .find(|d| d.header.vendor_id == vendor_id && d.header.device_id == device_id)
    }

    /// Perform passive read-only scan of a mapped ECAM segment buffer.
    ///
    /// # Safety
    /// `ecam_base_ptr` must point to valid mapped virtual memory for `segment`.
    pub unsafe fn scan_segment(&mut self, segment: &PciSegmentGroup, ecam_base_ptr: *const u8) {
        for bus in segment.start_bus_number..=segment.end_bus_number {
            for device in 0..32u8 {
                for function in 0..8u8 {
                    let bus_offset = (bus as usize - segment.start_bus_number as usize) << 20;
                    let dev_offset = (device as usize) << 15;
                    let func_offset = (function as usize) << 12;
                    let cfg_offset = bus_offset | dev_offset | func_offset;

                    // SAFETY: Reading 32-bit aligned vendor ID register from mapped ECAM memory.
                    let vendor_id = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset) as *const u16;
                        ptr.read_volatile()
                    };

                    if vendor_id == 0xFFFF || vendor_id == 0x0000 {
                        // Function not present
                        if function == 0 {
                            break; // Skip remaining functions if function 0 is absent
                        }
                        continue;
                    }

                    // SAFETY: Reading device ID register from mapped ECAM memory.
                    let device_id = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset + 2) as *const u16;
                        ptr.read_volatile()
                    };
                    // SAFETY: Reading command register from mapped ECAM memory.
                    let command = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset + 4) as *const u16;
                        ptr.read_volatile()
                    };
                    // SAFETY: Reading status register from mapped ECAM memory.
                    let status = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset + 6) as *const u16;
                        ptr.read_volatile()
                    };
                    // SAFETY: Reading revision and class code register from mapped ECAM memory.
                    let rev_prog_sub_class = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset + 8) as *const u32;
                        ptr.read_volatile()
                    };
                    let revision_id = (rev_prog_sub_class & 0xFF) as u8;
                    let prog_if = ((rev_prog_sub_class >> 8) & 0xFF) as u8;
                    let subclass = ((rev_prog_sub_class >> 16) & 0xFF) as u8;
                    let class_code = ((rev_prog_sub_class >> 24) & 0xFF) as u8;

                    // SAFETY: Reading header type register from mapped ECAM memory.
                    let header_type = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset + 14);
                        ptr.read_volatile()
                    };

                    // Read raw BARs (0x10..0x24) passively
                    let mut raw_bars = [0u32; 6];
                    for (bar_idx, bar_slot) in raw_bars.iter_mut().enumerate() {
                        // SAFETY: Reading raw BAR register from mapped ECAM memory.
                        *bar_slot = unsafe {
                            let ptr =
                                ecam_base_ptr.add(cfg_offset + 0x10 + bar_idx * 4) as *const u32;
                            ptr.read_volatile()
                        };
                    }

                    // SAFETY: Reading interrupt line register from mapped ECAM memory.
                    let interrupt_line = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset + 0x3C);
                        ptr.read_volatile()
                    };
                    // SAFETY: Reading interrupt pin register from mapped ECAM memory.
                    let interrupt_pin = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset + 0x3D);
                        ptr.read_volatile()
                    };
                    // SAFETY: Reading capability pointer register from mapped ECAM memory.
                    let capability_ptr = unsafe {
                        let ptr = ecam_base_ptr.add(cfg_offset + 0x34);
                        ptr.read_volatile()
                    };

                    let header = PciDeviceHeader {
                        vendor_id,
                        device_id,
                        command,
                        status,
                        revision_id,
                        prog_if,
                        subclass,
                        class_code,
                        header_type,
                        interrupt_line,
                        interrupt_pin,
                        capability_ptr,
                        raw_bars,
                    };

                    // Parse PCI Capability List if bit 4 of status is set
                    let mut capabilities = Vec::new();
                    if (status & (1 << 4)) != 0 && capability_ptr != 0 {
                        let mut curr_ptr = capability_ptr & 0xFC; // 4-byte aligned offset
                        let mut cap_count = 0;
                        while (0x40..0xFF).contains(&curr_ptr) && cap_count < 32 {
                            // SAFETY: Reading capability ID from mapped ECAM memory.
                            let cap_id = unsafe {
                                let ptr = ecam_base_ptr.add(cfg_offset + curr_ptr as usize);
                                ptr.read_volatile()
                            };
                            // SAFETY: Reading next capability pointer from mapped ECAM memory.
                            let next_ptr = unsafe {
                                let ptr = ecam_base_ptr.add(cfg_offset + curr_ptr as usize + 1);
                                ptr.read_volatile()
                            };

                            capabilities.push(PciCapabilityEntry {
                                cap_id,
                                offset: curr_ptr,
                                next_offset: next_ptr,
                                reserved: 0,
                            });

                            if next_ptr == 0 || next_ptr == curr_ptr {
                                break;
                            }
                            curr_ptr = next_ptr & 0xFC;
                            cap_count += 1;
                        }
                    }

                    self.devices.push(DiscoveredPciDevice {
                        segment: segment.segment_group_number,
                        bus,
                        device,
                        function,
                        header,
                        capabilities,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pci_bus_server_scan_segment_mock() {
        let segment = PciSegmentGroup {
            base_address: 0xE000_0000,
            segment_group_number: 0,
            start_bus_number: 0,
            end_bus_number: 0,
            reserved: 0,
        };

        // Create 1 MB mock ECAM buffer (256 KB per bus x 1 bus = 1 MB)
        let mut mock_ecam = alloc::vec![0u8; 1024 * 1024];

        // Populate mock device 0, func 0 with Vendor ID 0x1AF4 (VirtIO) and Device ID 0x1000
        mock_ecam[0..2].copy_from_slice(&0x1AF4u16.to_le_bytes());
        mock_ecam[2..4].copy_from_slice(&0x1000u16.to_le_bytes());

        let mut server = PciBusServer::new();
        // SAFETY: mock_ecam is allocated and valid for 1 MB.
        unsafe {
            server.scan_segment(&segment, mock_ecam.as_ptr());
        }

        assert_eq!(server.devices().len(), 1);
        let dev = &server.devices()[0];
        assert_eq!(dev.header.vendor_id, 0x1AF4);
        assert_eq!(dev.header.device_id, 0x1000);
        assert_eq!(dev.bus, 0);
        assert_eq!(dev.device, 0);
        assert_eq!(dev.function, 0);

        let found = server.find_device(0x1AF4, 0x1000).unwrap();
        assert_eq!(found.header.vendor_id, 0x1AF4);
        assert!(server.find_device(0x1AF4, 0x9999).is_none());
    }
}
