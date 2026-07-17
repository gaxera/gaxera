use core::arch::x86_64::__cpuid;
use core::fmt;
use core::ptr::write_volatile;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use x86_64::instructions::port::PortWriteOnly;
use x86_64::registers::model_specific::{ApicBase, ApicBaseFlags, Msr};
use x86_64::structures::paging::{FrameAllocator, Size4KiB};

use crate::arch::x86_64::acpi::LocalApicInfo;
use crate::arch::x86_64::paging::{KernelPageTables, PagingError};
use crate::memory::boot::BootContext;
use crate::memory::mapping::LOCAL_APIC_WINDOW;

pub const TIMER_VECTOR: u8 = 0xe0;
pub const SPURIOUS_VECTOR: u8 = 0xff;

const IA32_PAT: u32 = 0x277;
const PAT_UNCACHEABLE: u8 = 0x00;
const PAT_WRITE_BACK: u8 = 0x06;
const APIC_REGISTER_EOI: u64 = 0x0b0;
const APIC_REGISTER_SPURIOUS_INTERRUPT: u64 = 0x0f0;
const APIC_REGISTER_LVT_TIMER: u64 = 0x320;
const APIC_REGISTER_TIMER_INITIAL_COUNT: u64 = 0x380;
const APIC_REGISTER_TIMER_DIVIDE_CONFIGURATION: u64 = 0x3e0;
const APIC_SOFTWARE_ENABLE: u32 = 1 << 8;
const APIC_TIMER_MASKED: u32 = 1 << 16;
const APIC_TIMER_PERIODIC: u32 = 1 << 17;
const APIC_TIMER_DIVIDE_BY_16: u32 = 0b0011;
const TIMER_TEST_INITIAL_COUNT: u32 = 1_000_000;
const MASTER_PIC_DATA_PORT: u16 = 0x21;
const SLAVE_PIC_DATA_PORT: u16 = 0xa1;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static APIC_VIRTUAL_ADDRESS: AtomicU64 = AtomicU64::new(0);
static TIMER_TICKS: AtomicU64 = AtomicU64::new(0);
static TIMER_TARGET: AtomicU64 = AtomicU64::new(0);
static TIMER_TEST_COMPLETE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalApic {
    physical_address: u64,
}

impl LocalApic {
    pub const fn physical_address(self) -> u64 {
        self.physical_address
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalApicError {
    AlreadyInitialized,
    CpuDoesNotExposeLeafOne,
    CpuDoesNotSupportLocalApic,
    CpuDoesNotSupportPat,
    NotBootstrapProcessor,
    X2ApicAlreadyEnabled,
    FirmwareAndMadtAddressDisagree,
    PatEntryZeroIsNotWriteBack { actual: u8 },
    PatEntryThreeIsNotUncacheable { actual: u8 },
    TimerTargetIsZero,
    NotInitialized,
    Paging(PagingError),
}

impl From<PagingError> for LocalApicError {
    fn from(error: PagingError) -> Self {
        Self::Paging(error)
    }
}

impl fmt::Display for LocalApicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyInitialized => f.write_str("Local APIC initialized twice"),
            Self::CpuDoesNotExposeLeafOne => {
                f.write_str("CPU does not expose CPUID leaf 1 required for Local APIC")
            }
            Self::CpuDoesNotSupportLocalApic => f.write_str("CPU does not support a Local APIC"),
            Self::CpuDoesNotSupportPat => {
                f.write_str("CPU does not support the page attribute table")
            }
            Self::NotBootstrapProcessor => {
                f.write_str("Phase 5 supports Local APIC initialization on the BSP only")
            }
            Self::X2ApicAlreadyEnabled => {
                f.write_str("firmware enabled x2APIC but Phase 5 supports xAPIC MMIO only")
            }
            Self::FirmwareAndMadtAddressDisagree => {
                f.write_str("IA32_APIC_BASE disagrees with the MADT Local APIC address")
            }
            Self::PatEntryZeroIsNotWriteBack { actual } => write!(
                f,
                "PAT entry 0 is {actual:#04x}, not the WB memory type required for firmware reads"
            ),
            Self::PatEntryThreeIsNotUncacheable { actual } => write!(
                f,
                "PAT entry 3 is {actual:#04x}, not the UC memory type required for Local APIC MMIO"
            ),
            Self::TimerTargetIsZero => f.write_str("Local APIC timer target must be non-zero"),
            Self::NotInitialized => f.write_str("Local APIC has not been initialized"),
            Self::Paging(error) => error.fmt(f),
        }
    }
}

/// Validate and take sole Gaxera ownership of the BSP's xAPIC MMIO page.
///
/// # Safety
/// The Gaxera-owned CR3 must be active; interrupts must be disabled; the IDT
/// must already contain the timer and spurious-vector gates. No other code may
/// map or access the selected Local APIC page. The frame allocator must provide
/// page-table frames covered by Gaxera's RAM-only HHDM.
pub unsafe fn initialize<A>(
    context: &BootContext,
    info: LocalApicInfo,
    page_tables: &mut KernelPageTables,
    allocator: &mut A,
) -> Result<LocalApic, LocalApicError>
where
    A: FrameAllocator<Size4KiB>,
{
    if INITIALIZED.load(Ordering::Acquire) {
        return Err(LocalApicError::AlreadyInitialized);
    }
    validate_cpu_features()?;
    validate_pat_entries()?;

    let (base_frame, base_flags) = ApicBase::read();
    if !base_flags.contains(ApicBaseFlags::BSP) {
        return Err(LocalApicError::NotBootstrapProcessor);
    }
    if base_flags.contains(ApicBaseFlags::X2APIC_ENABLE) {
        return Err(LocalApicError::X2ApicAlreadyEnabled);
    }
    if base_frame.start_address().as_u64() != info.physical_address {
        return Err(LocalApicError::FirmwareAndMadtAddressDisagree);
    }

    // SAFETY: the preconditions of this function establish the single UC
    // mapping policy and the PAT verification required by this mapping API.
    unsafe {
        page_tables.map_local_apic_page(context, info.physical_address, allocator)?;
    }

    let mut enabled_flags = base_flags;
    enabled_flags.insert(ApicBaseFlags::LAPIC_ENABLE);
    // SAFETY: CPUID confirmed Local APIC support; the frame is the unchanged
    // firmware-selected base, and the flags preserve all reserved MSR fields.
    unsafe { ApicBase::write(base_frame, enabled_flags) };

    APIC_VIRTUAL_ADDRESS.store(LOCAL_APIC_WINDOW, Ordering::Release);
    // SAFETY: the permanent UC mapping is present, writable, and exclusive.
    // Masking the timer before `sti` prevents inherited firmware timer state
    // from delivering an interrupt outside Gaxera's explicit test setup.
    unsafe {
        write_register(
            APIC_REGISTER_LVT_TIMER,
            u32::from(TIMER_VECTOR) | APIC_TIMER_MASKED,
        );
        write_register(
            APIC_REGISTER_SPURIOUS_INTERRUPT,
            u32::from(SPURIOUS_VECTOR) | APIC_SOFTWARE_ENABLE,
        );
        mask_legacy_pics();
    }
    INITIALIZED.store(true, Ordering::Release);

    Ok(LocalApic {
        physical_address: info.physical_address,
    })
}

/// Arm a periodic timer solely for the deterministic Phase 5 delivery proof.
///
/// No frequency, duration, or clocksource claim is implied by this operation.
///
/// # Safety
/// The Local APIC must have been initialized by this module. Interrupts must
/// still be disabled while the counter, target, and LVT state are prepared.
pub unsafe fn start_periodic_timer_test(target: u64) -> Result<(), LocalApicError> {
    if !INITIALIZED.load(Ordering::Acquire) {
        return Err(LocalApicError::NotInitialized);
    }
    if target == 0 {
        return Err(LocalApicError::TimerTargetIsZero);
    }

    TIMER_TICKS.store(0, Ordering::Relaxed);
    TIMER_TEST_COMPLETE.store(false, Ordering::Relaxed);
    TIMER_TARGET.store(target, Ordering::Release);
    // SAFETY: only the BSP accesses this exclusive Local APIC page. The initial
    // count is deliberately written last, after the vector and periodic mode.
    unsafe {
        write_register(
            APIC_REGISTER_TIMER_DIVIDE_CONFIGURATION,
            APIC_TIMER_DIVIDE_BY_16,
        );
        write_register(
            APIC_REGISTER_LVT_TIMER,
            u32::from(TIMER_VECTOR) | APIC_TIMER_PERIODIC,
        );
        write_register(APIC_REGISTER_TIMER_INITIAL_COUNT, TIMER_TEST_INITIAL_COUNT);
    }
    Ok(())
}

pub fn timer_test_complete() -> bool {
    TIMER_TEST_COMPLETE.load(Ordering::Acquire)
}

pub fn timer_ticks() -> u64 {
    TIMER_TICKS.load(Ordering::Acquire)
}

#[cfg(feature = "test-apic-timer")]
pub fn run_timer_delivery_test() -> ! {
    const TARGET_TICKS: u64 = 3;

    // SAFETY: the caller reaches this function only after Phase 5 xAPIC
    // initialization, with interrupts still disabled and the timer gate live.
    if let Err(error) = unsafe { start_periodic_timer_test(TARGET_TICKS) } {
        crate::println!("GAXERA ERROR: APIC_TIMER_TEST_SETUP_FAILED: {error}");
        // SAFETY: this feature always runs under xtask's isa-debug-exit device.
        unsafe { crate::arch::x86_64::qemu::exit_failure() }
    }

    while !timer_test_complete() {
        x86_64::instructions::interrupts::enable_and_hlt();
    }

    let ticks = timer_ticks();
    if ticks != TARGET_TICKS {
        crate::println!(
            "GAXERA ERROR: APIC_TIMER_TEST_WRONG_TICK_COUNT expected={} actual={}",
            TARGET_TICKS,
            ticks
        );
        // SAFETY: this feature always runs under xtask's isa-debug-exit device.
        unsafe { crate::arch::x86_64::qemu::exit_failure() }
    }
    crate::println!("GAXERA: APIC_TIMER_DELIVERY_OK ticks={ticks}");
    // SAFETY: this feature always runs under xtask's isa-debug-exit device.
    unsafe { crate::arch::x86_64::qemu::exit_success() }
}

/// The sole Phase 5 Local APIC timer interrupt path.
///
/// This handler performs no allocation, printing, locking, mapping, or
/// scheduling. At the requested count it masks the LVT before EOI, so the test
/// observes an exact delivery count rather than a timing-dependent range.
pub fn on_timer_interrupt() {
    let tick = TIMER_TICKS.fetch_add(1, Ordering::Relaxed) + 1;
    let target = TIMER_TARGET.load(Ordering::Acquire);
    if target != 0 && tick == target {
        // SAFETY: only the BSP timer interrupt can reach this handler, and the
        // APIC page was published with Release before interrupts were enabled.
        unsafe {
            write_register(
                APIC_REGISTER_LVT_TIMER,
                u32::from(TIMER_VECTOR) | APIC_TIMER_MASKED,
            );
        }
        TIMER_TEST_COMPLETE.store(true, Ordering::Release);
    }
    // SAFETY: this handler is entered only for the Local APIC timer vector.
    // Acknowledging it after state updates permits the next interrupt only
    // after the LVT-mask decision above is visible to the device.
    unsafe { end_of_interrupt() };
}

/// Acknowledge a Local APIC interrupt after its handler has completed work.
///
/// # Safety
/// The caller must be running in a handler for a Local APIC-delivered vector.
pub(crate) unsafe fn end_of_interrupt() {
    // SAFETY: caller establishes that the Local APIC owns the current interrupt.
    unsafe { write_register(APIC_REGISTER_EOI, 0) };
}

fn validate_cpu_features() -> Result<(), LocalApicError> {
    // CPUID is architecturally available on every x86-64 processor.
    let maximum_basic_leaf = __cpuid(0).eax;
    if maximum_basic_leaf < 1 {
        return Err(LocalApicError::CpuDoesNotExposeLeafOne);
    }
    // The maximum leaf check above makes leaf 1 available.
    let leaf_one = __cpuid(1);
    if leaf_one.edx & (1 << 9) == 0 {
        return Err(LocalApicError::CpuDoesNotSupportLocalApic);
    }
    if leaf_one.edx & (1 << 16) == 0 {
        return Err(LocalApicError::CpuDoesNotSupportPat);
    }
    Ok(())
}

fn validate_pat_entries() -> Result<(), LocalApicError> {
    let pat = Msr::new(IA32_PAT);
    // SAFETY: CPUID.PAT was verified before this function is called.
    let value = unsafe { pat.read() };
    let entry_zero = pat_entry(value, 0);
    if entry_zero != PAT_WRITE_BACK {
        return Err(LocalApicError::PatEntryZeroIsNotWriteBack { actual: entry_zero });
    }
    let entry_three = pat_entry(value, 3);
    if entry_three != PAT_UNCACHEABLE {
        return Err(LocalApicError::PatEntryThreeIsNotUncacheable {
            actual: entry_three,
        });
    }
    Ok(())
}

const fn pat_entry(value: u64, index: u32) -> u8 {
    ((value >> (index * 8)) & 0xff) as u8
}

unsafe fn write_register(offset: u64, value: u32) {
    let base = APIC_VIRTUAL_ADDRESS.load(Ordering::Acquire);
    debug_assert_ne!(base, 0);
    // SAFETY: callers establish that this is the unique permanent UC mapping
    // and that `offset` names a 32-bit Local APIC register within its page.
    unsafe { write_volatile((base + offset) as *mut u32, value) };
}

unsafe fn mask_legacy_pics() {
    let mut master = PortWriteOnly::<u8>::new(MASTER_PIC_DATA_PORT);
    let mut slave = PortWriteOnly::<u8>::new(SLAVE_PIC_DATA_PORT);
    // SAFETY: Phase 5 takes BSP interrupt-routing ownership before `sti` and
    // masks every 8259 input. The PICs are never later used in this phase.
    unsafe {
        master.write(u8::MAX);
        slave.write(u8::MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::{PAT_UNCACHEABLE, PAT_WRITE_BACK, pat_entry};

    #[test]
    fn extracts_pat_entry_three() {
        let pat = 0x0007_0406_0007_0406_u64;
        assert_eq!(pat_entry(pat, 0), PAT_WRITE_BACK);
        assert_eq!(pat_entry(pat, 3), PAT_UNCACHEABLE);
        assert_eq!(pat_entry(pat, 2), 0x07);
    }
}
