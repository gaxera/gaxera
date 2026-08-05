#![allow(clippy::undocumented_unsafe_blocks)]

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
const IOAPIC_REG_VERSION: u8 = IOAPIC_REG_VER;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static IOAPIC_VIRT_ADDR: AtomicU64 = AtomicU64::new(0);
static GSI_BASE: AtomicU64 = AtomicU64::new(0);
static MAX_GSI: AtomicU64 = AtomicU64::new(0);
static ISA_IRQ_TO_GSI: [AtomicU64; 16] = [const { AtomicU64::new(0) }; 16];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IoApicError {
    AlreadyInitialized,
    InvalidAddress,
    NotInitialized,
    GsiOutOfRange,
}

pub fn ioapic_init(virt_addr: u64) {
    IOAPIC_VIRT_ADDR.store(virt_addr, Ordering::Release);
    INITIALIZED.store(true, Ordering::Release);
}

/// Take ownership of one firmware-described IOAPIC and mask every redirection
/// entry before any device capability is published.
pub fn initialize(
    virt_addr: u64,
    gsi_base: u32,
    isa_irq_overrides: &[u32; 16],
) -> Result<(), IoApicError> {
    if virt_addr == 0 || !virt_addr.is_multiple_of(4096) {
        return Err(IoApicError::InvalidAddress);
    }
    if INITIALIZED.swap(true, Ordering::AcqRel) {
        return Err(IoApicError::AlreadyInitialized);
    }
    ioapic_init(virt_addr);
    let version = unsafe { read_reg(IOAPIC_REG_VERSION) };
    let max_entry = ((version >> 16) & 0xff) as u64;
    if max_entry == 0 {
        INITIALIZED.store(false, Ordering::Release);
        return Err(IoApicError::InvalidAddress);
    }
    GSI_BASE.store(u64::from(gsi_base), Ordering::Release);
    MAX_GSI.store(u64::from(gsi_base) + max_entry, Ordering::Release);
    for (irq, gsi) in isa_irq_overrides.iter().enumerate() {
        ISA_IRQ_TO_GSI[irq].store(u64::from(*gsi), Ordering::Release);
    }
    for entry in 0..=max_entry {
        let gsi = gsi_base.saturating_add(entry as u32);
        let _ = mask_gsi(gsi);
    }
    Ok(())
}

pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

pub fn gsi_range() -> Option<(u32, u32)> {
    if !is_initialized() {
        return None;
    }
    Some((
        GSI_BASE.load(Ordering::Acquire) as u32,
        MAX_GSI.load(Ordering::Acquire) as u32,
    ))
}

pub fn gsi_to_redirection(gsi: u32) -> Result<u8, IoApicError> {
    if !is_initialized() {
        return Err(IoApicError::NotInitialized);
    }
    let base = GSI_BASE.load(Ordering::Acquire) as u32;
    let max = MAX_GSI.load(Ordering::Acquire) as u32;
    if gsi < base || gsi > max {
        return Err(IoApicError::GsiOutOfRange);
    }
    u8::try_from(gsi - base).map_err(|_| IoApicError::GsiOutOfRange)
}

pub fn isa_irq_to_gsi(irq: u8) -> Result<u32, IoApicError> {
    let index = usize::from(irq);
    let gsi = ISA_IRQ_TO_GSI
        .get(index)
        .ok_or(IoApicError::GsiOutOfRange)?
        .load(Ordering::Acquire) as u32;
    if gsi_to_redirection(gsi).is_err() {
        return Err(IoApicError::GsiOutOfRange);
    }
    Ok(gsi)
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
    ioapic_set_redirection_with_trigger(irq, vector, dest_apic_id, masked, false, false);
}

/// Program a redirection entry with explicit electrical semantics.
///
/// PCI INTx is level-triggered and active-low. ISA edge-triggered sources
/// should continue to use `ioapic_set_redirection`, which preserves the
/// default active-high edge configuration.
pub fn ioapic_set_redirection_with_trigger(
    irq: u8,
    vector: u8,
    dest_apic_id: u8,
    masked: bool,
    level_triggered: bool,
    active_low: bool,
) {
    let Ok(gsi) = isa_irq_to_gsi(irq) else {
        return;
    };
    let Ok(redirection) = gsi_to_redirection(gsi) else {
        return;
    };
    let reg_low = IOAPIC_REG_REDTBL_BASE + redirection * 2;
    let reg_high = reg_low + 1;

    let mut low = vector as u32; // Fixed delivery, physical dest
    if masked {
        low |= 1 << 16;
    }
    if active_low {
        low |= 1 << 13;
    }
    if level_triggered {
        low |= 1 << 15;
    }

    let high = (dest_apic_id as u32) << 24;

    // SAFETY: Single-core BSP or atomic IOAPIC programming scope.
    unsafe {
        write_reg(reg_low, low);
        write_reg(reg_high, high);
    }
}

pub fn ioapic_mask_irq(irq: u8) {
    let Ok(gsi) = isa_irq_to_gsi(irq) else {
        return;
    };
    let Ok(redirection) = gsi_to_redirection(gsi) else {
        return;
    };
    let reg_low = IOAPIC_REG_REDTBL_BASE + redirection * 2;
    // SAFETY: MMIO access to redirection table entry.
    unsafe {
        let low = read_reg(reg_low);
        write_reg(reg_low, low | (1 << 16));
    }
}

pub fn ioapic_unmask_irq(irq: u8) {
    let Ok(gsi) = isa_irq_to_gsi(irq) else {
        return;
    };
    let Ok(redirection) = gsi_to_redirection(gsi) else {
        return;
    };
    let reg_low = IOAPIC_REG_REDTBL_BASE + redirection * 2;
    // SAFETY: MMIO access to redirection table entry.
    unsafe {
        let low = read_reg(reg_low);
        write_reg(reg_low, low & !(1 << 16));
    }
}

fn mask_gsi(gsi: u32) -> Result<(), IoApicError> {
    let redirection = gsi_to_redirection(gsi)?;
    let reg_low = IOAPIC_REG_REDTBL_BASE + redirection * 2;
    // SAFETY: MMIO access to a validated IOAPIC redirection entry.
    unsafe {
        let low = read_reg(reg_low);
        write_reg(reg_low, low | (1 << 16));
    }
    Ok(())
}
