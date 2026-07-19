#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootInfo {
    pub magic: u64,
    pub abi_version: u32,
    pub reserved: u32,
}

impl BootInfo {
    pub const MAGIC: u64 = 0x676178657261; // 'gaxera' in hex
    pub const ABI_VERSION: u32 = 1;
}
