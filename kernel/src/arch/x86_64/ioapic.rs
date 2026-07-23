use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[allow(dead_code)]
const IOREGSEL_OFFSET: u64 = 0x00;
const IOWIN_OFFSET: u64 = 0x10;

#[allow(dead_code)]
const IOAPIC_REG_ID: u8 = 0x00;
#[allow(dead_code)]
const IOAPIC_REG_VER: u8 = 0x01;
const IOAPIC_REG_REDTBL_BASE: u8 = 0x10;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static IOAPIC_VIRT_ADDR: AtomicU64 = AtomicU64::new(0);

pub fn ioapic_init(virt_addr: u64) {
    IOAPIC_VIRT_ADDR.store(virt_addr, Ordering::Release);
    INITIALIZED.store(true, Ordering::Release);
}

pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

unsafe fn read_reg(reg: u8) -> u32 {
    let base = IOAPIC_VIRT_ADDR.load(Ordering::Acquire);
    if base == 0 {
        return 0;
    }
    // SAFETY: caller ensures virt_addr points to valid IOAPIC MMIO window.
    unsafe {
        write_volatile(base as *mut u32, reg as u32);
        read_volatile((base + IOWIN_OFFSET) as *const u32)
    }
}

unsafe fn write_reg(reg: u8, value: u32) {
    let base = IOAPIC_VIRT_ADDR.load(Ordering::Acquire);
    if base == 0 {
        return;
    }
    // SAFETY: caller ensures virt_addr points to valid IOAPIC MMIO window.
    unsafe {
        write_volatile(base as *mut u32, reg as u32);
        write_volatile((base + IOWIN_OFFSET) as *mut u32, value);
    }
}

pub fn ioapic_set_redirection(irq: u8, vector: u8, dest_apic_id: u8, masked: bool) {
    let reg_low = IOAPIC_REG_REDTBL_BASE + irq * 2;
    let reg_high = reg_low + 1;

    let mut low = vector as u32; // Fixed delivery, physical dest
    if masked {
        low |= 1 << 16;
    }

    let high = (dest_apic_id as u32) << 24;

    // SAFETY: Single-core BSP or atomic IOAPIC programming scope.
    unsafe {
        write_reg(reg_low, low);
        write_reg(reg_high, high);
    }
}

pub fn ioapic_mask_irq(irq: u8) {
    let reg_low = IOAPIC_REG_REDTBL_BASE + irq * 2;
    // SAFETY: MMIO access to redirection table entry.
    unsafe {
        let low = read_reg(reg_low);
        write_reg(reg_low, low | (1 << 16));
    }
}

pub fn ioapic_unmask_irq(irq: u8) {
    let reg_low = IOAPIC_REG_REDTBL_BASE + irq * 2;
    // SAFETY: MMIO access to redirection table entry.
    unsafe {
        let low = read_reg(reg_low);
        write_reg(reg_low, low & !(1 << 16));
    }
}
