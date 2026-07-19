pub const HHDM_BASE: u64 = 0xffff_8000_0000_0000;
pub const HHDM_PHYSICAL_LIMIT: u64 = 126 * 1024 * 1024 * 1024 * 1024;

// M2A uses one deterministic lower-half probe layout. These are not a general
// user VM allocator or a stable user ABI.
pub const USER_NULL_GUARD: u64 = 0x0000_0000_0000_0000;
pub const USER_PROBE_CODE: u64 = 0x0000_0000_0040_0000;
pub const USER_STACK_PAGE: u64 = 0x0000_0000_0080_0000;
pub const USER_STACK_TOP: u64 = 0x0000_0000_0080_1000;
pub const USER_STACK_UPPER_GUARD: u64 = USER_STACK_TOP;
pub const USER_ADDRESS_MAX: u64 = 0x0000_7FFF_FFFF_FFFF;

pub const FRAMEBUFFER_BASE: u64 = 0xffff_fe00_0000_0000;
pub const HEAP_LOWER_GUARD: u64 = 0xffff_fe80_0000_0000;
pub const HEAP_START: u64 = HEAP_LOWER_GUARD + 4096;
pub const HEAP_SIZE: u64 = 2 * 1024 * 1024;
pub const HEAP_UPPER_GUARD: u64 = HEAP_START + HEAP_SIZE;
pub const ACPI_TABLE_WINDOW: u64 = 0xffff_fe90_0000_0000;
pub const LOCAL_APIC_WINDOW: u64 = 0xffff_fea0_0000_0000;

pub const KERNEL_VIRTUAL_BASE: u64 = 0xffff_ffff_8000_0000;
