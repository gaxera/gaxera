pub const HHDM_BASE: u64 = 0xffff_8000_0000_0000;
pub const HHDM_PHYSICAL_LIMIT: u64 = 126 * 1024 * 1024 * 1024 * 1024;

pub const FRAMEBUFFER_BASE: u64 = 0xffff_fe00_0000_0000;
pub const HEAP_LOWER_GUARD: u64 = 0xffff_fe80_0000_0000;
pub const HEAP_START: u64 = HEAP_LOWER_GUARD + 4096;
pub const HEAP_SIZE: u64 = 2 * 1024 * 1024;
pub const HEAP_UPPER_GUARD: u64 = HEAP_START + HEAP_SIZE;

pub const KERNEL_VIRTUAL_BASE: u64 = 0xffff_ffff_8000_0000;
