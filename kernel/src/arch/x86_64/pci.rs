use alloc::vec::Vec;
use gaxera_abi::pci::PciSegmentGroup;

pub const MCFG_SIGNATURE: &[u8; 4] = b"MCFG";

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
