#![allow(clippy::undocumented_unsafe_blocks)]

use alloc::vec::Vec;
use gaxera_abi::pci::PciSegmentGroup;

pub const MCFG_SIGNATURE: &[u8; 4] = b"MCFG";

pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;
pub const VIRTIO_RNG_TRANSITIONAL_DEVICE_ID: u16 = 0x1005;
pub const VIRTIO_RNG_MODERN_DEVICE_ID: u16 = 0x1044;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PciBarWindow {
    pub physical_base: u64,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtioPciRegion {
    pub cfg_type: u8,
    pub bar: u8,
    pub offset: u32,
    pub length: u32,
    pub notify_off_multiplier: u32,
    pub window: PciBarWindow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VirtioRngPciInfo {
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub interrupt_line: u8,
    pub common: VirtioPciRegion,
    pub notify: VirtioPciRegion,
    pub isr: VirtioPciRegion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PciDiscoveryError {
    NoDevice,
    InvalidBar,
    MissingVirtioCapability(u8),
    InvalidCapability,
    InvalidInterruptLine,
}

const VIRTIO_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_CAP_NOTIFY_CFG: u8 = 2;
const VIRTIO_CAP_ISR_CFG: u8 = 3;
const VIRTIO_VENDOR_CAP_ID: u8 = 0x09;

/// Discover a QEMU VirtIO-RNG PCI function using firmware-provided ECAM.
///
/// This routine only reads PCI configuration and enables memory/bus mastering
/// on the selected function. It does not expose a physical address to user
/// space; the returned windows are consumed by the kernel bootstrap authority
/// layer, which creates explicit Mapping capabilities for the driver.
///
/// # Safety
/// The ECAM ranges must be mapped by the active HHDM and contain valid PCI
/// configuration space. The caller must own PCI discovery during bootstrap.
pub unsafe fn discover_virtio_rng(
    segments: &[PciSegmentGroup],
    ecam_window: u64,
) -> Result<VirtioRngPciInfo, PciDiscoveryError> {
    for segment in segments {
        for bus in segment.start_bus_number..=segment.end_bus_number {
            for device in 0..32_u8 {
                for function in 0..8_u8 {
                    let offset = ((bus - segment.start_bus_number) as u64) << 20
                        | (u64::from(device) << 15)
                        | (u64::from(function) << 12);
                    let ecam = ecam_window
                        .checked_add(offset)
                        .ok_or(PciDiscoveryError::InvalidCapability)?;
                    let vendor = unsafe { read_u16(ecam, 0x00) };
                    if vendor == 0xFFFF || vendor == 0 {
                        if function == 0 {
                            break;
                        }
                        continue;
                    }
                    let device_id = unsafe { read_u16(ecam, 0x02) };
                    if vendor != VIRTIO_VENDOR_ID
                        || (device_id != VIRTIO_RNG_TRANSITIONAL_DEVICE_ID
                            && device_id != VIRTIO_RNG_MODERN_DEVICE_ID)
                    {
                        if function == 0 && unsafe { read_u8(ecam, 0x0E) } & 0x80 == 0 {
                            break;
                        }
                        continue;
                    }

                    let command = unsafe { read_u16(ecam, 0x04) } | (1 << 1) | (1 << 2);
                    unsafe { write_u16(ecam, 0x04, command) };
                    let interrupt_line = unsafe { read_u8(ecam, 0x3C) };
                    if interrupt_line == 0 || interrupt_line >= 16 {
                        return Err(PciDiscoveryError::InvalidInterruptLine);
                    }

                    let mut bars = [None; 6];
                    let mut bar = 0;
                    while bar < 6 {
                        let raw = unsafe { read_u32(ecam, 0x10 + bar * 4) };
                        // A zero BAR is an unassigned optional BAR, not a malformed
                        // device.  QEMU's virtio-rng exposes only one memory BAR.
                        if raw == 0 {
                            bar += 1;
                            continue;
                        }
                        if raw & 1 == 1 {
                            bar += 1;
                            continue;
                        }
                        let kind = (raw >> 1) & 0x3;
                        let is_64_bit = kind == 0x2 && bar < 5;
                        let original_high = if is_64_bit {
                            Some(unsafe { read_u32(ecam, 0x14 + bar * 4) })
                        } else {
                            None
                        };
                        let base =
                            u64::from(raw & !0xF) | (u64::from(original_high.unwrap_or(0)) << 32);

                        // PCI BAR sizing is defined by writing all ones, reading the
                        // implemented address mask, then restoring the original BAR.
                        // This is required because vendor capabilities may extend past
                        // the first page of a device's MMIO aperture.
                        unsafe { write_u32(ecam, 0x10 + bar * 4, u32::MAX) };
                        let size_low = unsafe { read_u32(ecam, 0x10 + bar * 4) };
                        unsafe { write_u32(ecam, 0x10 + bar * 4, raw) };
                        let size_high = if is_64_bit {
                            unsafe { write_u32(ecam, 0x14 + bar * 4, u32::MAX) };
                            let mask = unsafe { read_u32(ecam, 0x14 + bar * 4) };
                            unsafe { write_u32(ecam, 0x14 + bar * 4, original_high.unwrap()) };
                            mask
                        } else {
                            0
                        };
                        let size = if is_64_bit {
                            let size_mask =
                                u64::from(size_low & !0xF) | (u64::from(size_high) << 32);
                            (!size_mask).wrapping_add(1)
                        } else {
                            u64::from((!(size_low & !0xF)).wrapping_add(1))
                        };

                        if base == 0
                            || !base.is_multiple_of(4096)
                            || size == 0
                            || !size.is_power_of_two()
                        {
                            return Err(PciDiscoveryError::InvalidBar);
                        }
                        bars[bar as usize] = Some(PciBarWindow {
                            physical_base: base,
                            size,
                        });
                        if is_64_bit {
                            bar += 1;
                        }
                        bar += 1;
                    }

                    let status = unsafe { read_u16(ecam, 0x06) };
                    let mut cap = if status & (1 << 4) != 0 {
                        (unsafe { read_u8(ecam, 0x34) }) & 0xFC
                    } else {
                        0
                    };
                    let mut common = None;
                    let mut notify = None;
                    let mut isr = None;
                    let mut seen = [false; 256];
                    for _ in 0..32 {
                        if !(0x40..=0xFC).contains(&cap) || seen[cap as usize] {
                            break;
                        }
                        seen[cap as usize] = true;
                        let cap_id = unsafe { read_u8(ecam, u64::from(cap)) };
                        let next = unsafe { read_u8(ecam, u64::from(cap) + 1) } & 0xFC;
                        let cap_len = unsafe { read_u8(ecam, u64::from(cap) + 2) };
                        if cap_id == VIRTIO_VENDOR_CAP_ID && cap_len >= 16 {
                            let cfg_type = unsafe { read_u8(ecam, u64::from(cap) + 3) };
                            let cap_bar = unsafe { read_u8(ecam, u64::from(cap) + 4) };
                            let cap_offset = unsafe { read_u32(ecam, u64::from(cap) + 8) };
                            let cap_length = unsafe { read_u32(ecam, u64::from(cap) + 12) };
                            let multiplier = if cfg_type == VIRTIO_CAP_NOTIFY_CFG && cap_len >= 20 {
                                unsafe { read_u32(ecam, u64::from(cap) + 16) }
                            } else {
                                0
                            };
                            if !matches!(
                                cfg_type,
                                VIRTIO_CAP_COMMON_CFG | VIRTIO_CAP_NOTIFY_CFG | VIRTIO_CAP_ISR_CFG
                            ) {
                                if next == 0 {
                                    break;
                                }
                                cap = next;
                                continue;
                            }
                            let window = bars
                                .get(cap_bar as usize)
                                .and_then(|window| *window)
                                .ok_or(PciDiscoveryError::InvalidCapability)?;
                            if cap_length == 0
                                || u64::from(cap_offset)
                                    .checked_add(u64::from(cap_length))
                                    .is_none_or(|end| end > window.size)
                            {
                                return Err(PciDiscoveryError::InvalidCapability);
                            }
                            let region = VirtioPciRegion {
                                cfg_type,
                                bar: cap_bar,
                                offset: cap_offset,
                                length: cap_length,
                                notify_off_multiplier: multiplier,
                                window,
                            };
                            match cfg_type {
                                VIRTIO_CAP_COMMON_CFG => common = Some(region),
                                VIRTIO_CAP_NOTIFY_CFG => notify = Some(region),
                                VIRTIO_CAP_ISR_CFG => isr = Some(region),
                                _ => {}
                            }
                        }
                        if next == 0 {
                            break;
                        }
                        cap = next;
                    }
                    return Ok(VirtioRngPciInfo {
                        segment: segment.segment_group_number,
                        bus,
                        device,
                        function,
                        interrupt_line,
                        common: common.ok_or(PciDiscoveryError::MissingVirtioCapability(
                            VIRTIO_CAP_COMMON_CFG,
                        ))?,
                        notify: notify.ok_or(PciDiscoveryError::MissingVirtioCapability(
                            VIRTIO_CAP_NOTIFY_CFG,
                        ))?,
                        isr: isr.ok_or(PciDiscoveryError::MissingVirtioCapability(
                            VIRTIO_CAP_ISR_CFG,
                        ))?,
                    });
                }
            }
        }
    }
    Err(PciDiscoveryError::NoDevice)
}

unsafe fn read_u8(base: u64, offset: u64) -> u8 {
    // SAFETY: caller validates the ECAM window and register offset.
    unsafe { ((base + offset) as *const u8).read_volatile() }
}

unsafe fn read_u16(base: u64, offset: u64) -> u16 {
    // SAFETY: caller validates the ECAM window and alignment.
    unsafe { ((base + offset) as *const u16).read_volatile() }
}

unsafe fn read_u32(base: u64, offset: u64) -> u32 {
    // SAFETY: caller validates the ECAM window and alignment.
    unsafe { ((base + offset) as *const u32).read_volatile() }
}

unsafe fn write_u16(base: u64, offset: u64, value: u16) {
    // SAFETY: caller validates the ECAM window and alignment.
    unsafe { ((base + offset) as *mut u16).write_volatile(value) }
}

unsafe fn write_u32(base: u64, offset: u64, value: u32) {
    // SAFETY: caller validates the ECAM window and register alignment.
    unsafe { ((base + offset) as *mut u32).write_volatile(value) }
}

/// Parse ACPI MCFG table bytes to discover all PCIe ECAM segment groups.
pub fn parse_mcfg_segments(mcfg_bytes: &[u8]) -> Option<Vec<PciSegmentGroup>> {
    if mcfg_bytes.len() < 44 || &mcfg_bytes[..4] != MCFG_SIGNATURE {
        return None;
    }

    let mut segments = Vec::new();
    let mut offset = 44; // MCFG header length: 36 SDT header + 8 reserved bytes

    while offset + 16 <= mcfg_bytes.len() {
        let base_address = u64::from_le_bytes(mcfg_bytes[offset..offset + 8].try_into().ok()?);
        let segment_group_number =
            u16::from_le_bytes(mcfg_bytes[offset + 8..offset + 10].try_into().ok()?);
        let start_bus_number = mcfg_bytes[offset + 10];
        let end_bus_number = mcfg_bytes[offset + 11];

        segments.push(PciSegmentGroup {
            base_address,
            segment_group_number,
            start_bus_number,
            end_bus_number,
            reserved: 0,
        });

        offset += 16;
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_parse_mcfg_segments() {
        let mut mcfg = vec![0u8; 60];
        mcfg[..4].copy_from_slice(b"MCFG");
        mcfg[4..8].copy_from_slice(&60u32.to_le_bytes());

        // Segment 0: base=0xE0000000, seg=0, start=0, end=255
        mcfg[44..52].copy_from_slice(&0xE000_0000u64.to_le_bytes());
        mcfg[52..54].copy_from_slice(&0u16.to_le_bytes());
        mcfg[54] = 0;
        mcfg[55] = 255;

        let segments = parse_mcfg_segments(&mcfg).expect("mcfg parse failed");
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].base_address, 0xE000_0000);
        assert_eq!(segments[0].start_bus_number, 0);
        assert_eq!(segments[0].end_bus_number, 255);
    }
}
