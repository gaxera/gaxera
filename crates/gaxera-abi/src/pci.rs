/// Single PCIe ECAM segment group description parsed from ACPI MCFG table.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct PciSegmentGroup {
    pub base_address: u64,
    pub segment_group_number: u16,
    pub start_bus_number: u8,
    pub end_bus_number: u8,
    pub reserved: u32,
}

/// PCI Device Header summary parsed passively from configuration space.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct PciDeviceHeader {
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub header_type: u8,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub capability_ptr: u8,
    pub raw_bars: [u32; 6],
}

/// PCI Capability linked list node descriptor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct PciCapabilityEntry {
    pub cap_id: u8,
    pub offset: u8,
    pub next_offset: u8,
    pub reserved: u8,
}

/// Standardized PCI Bus Server Operation Codes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum PciOp {
    QueryDevice = 1,
    GrantDeviceBar = 2,
}

/// Device lookup request by Vendor ID and Device ID.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct PciQueryReq {
    pub vendor_id: u16,
    pub device_id: u16,
}

/// Device lookup response summary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct PciQueryResp {
    pub status: u32,
    pub segment: u16,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub reserved: u8,
    pub header: PciDeviceHeader,
}
