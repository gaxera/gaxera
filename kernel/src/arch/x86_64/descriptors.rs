use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;
const DOUBLE_FAULT_STACK_SIZE: usize = 32 * 1024;

#[repr(align(16))]
struct Stack([u8; DOUBLE_FAULT_STACK_SIZE]);

pub(crate) struct StaticCell<T>(UnsafeCell<T>);

// SAFETY: Phase 3 initializes descriptor state once on the bootstrap CPU with
// interrupts disabled. No references to the contained values escape, and later
// phases must introduce explicit per-CPU synchronization before sharing them.
unsafe impl<T: Send> Sync for StaticCell<T> {}

impl<T> StaticCell<T> {
    pub(crate) const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    /// # Safety
    /// The caller must uphold the single-writer initialization rule documented
    /// on the owning static allocation.
    pub(crate) unsafe fn get(&self) -> *mut T {
        self.0.get()
    }
}

struct DescriptorTables {
    gdt: GlobalDescriptorTable,
    tss: TaskStateSegment,
}

impl DescriptorTables {
    const fn new() -> Self {
        Self {
            gdt: GlobalDescriptorTable::new(),
            tss: TaskStateSegment::new(),
        }
    }
}

static TABLES: StaticCell<DescriptorTables> = StaticCell::new(DescriptorTables::new());
#[unsafe(link_section = ".ist_stack")]
#[used]
static DOUBLE_FAULT_STACK: StaticCell<Stack> = StaticCell::new(Stack([0; DOUBLE_FAULT_STACK_SIZE]));
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Install Gaxera-owned GDT and TSS state, including the double-fault IST stack.
///
/// # Safety
/// This must run exactly once on the bootstrap CPU before interrupts are enabled.
/// The Limine entry contract supplies an initially valid stack and GDT, allowing
/// this function to reload the segment state safely.
pub unsafe fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        panic!("descriptor tables initialized twice");
    }

    // SAFETY: This is the one initialization path. `TABLES` and the IST stack
    // have static storage, remain at fixed addresses, and are never mutated
    // after the GDT and TSS become active.
    let tables = unsafe { &mut *TABLES.get() };
    let stack = unsafe { &mut *DOUBLE_FAULT_STACK.get() };

    let stack_top = VirtAddr::from_ptr(stack.0.as_mut_ptr().wrapping_add(DOUBLE_FAULT_STACK_SIZE));
    tables.tss.interrupt_stack_table[usize::from(DOUBLE_FAULT_IST_INDEX)] = stack_top;

    let code_selector = tables.gdt.append(Descriptor::kernel_code_segment());
    let data_selector = tables.gdt.append(Descriptor::kernel_data_segment());
    let tss_selector = tables
        .gdt
        // SAFETY: `tables.tss` has static storage inside `TABLES` and is not
        // moved or modified after this descriptor is loaded.
        .append(unsafe { Descriptor::tss_segment_unchecked(&raw const tables.tss) });

    load_gdt_and_segments(tables, code_selector, data_selector, tss_selector);
}

/// Return whether `stack_pointer` is within the double-fault IST allocation.
///
/// The stack grows down, so the initial TSS pointer is one byte past `end` and
/// an entered handler must have an RSP strictly below that value. This function
/// reads only the static allocation's address; the allocation is immutable
/// after `init` completes.
pub(crate) fn is_on_double_fault_stack(stack_pointer: u64) -> bool {
    // SAFETY: `DOUBLE_FAULT_STACK` is fully initialized before the IDT becomes
    // active and is never modified afterward. Taking its address does not
    // create an alias to mutable descriptor state.
    let start = unsafe { (*DOUBLE_FAULT_STACK.get()).0.as_ptr() as u64 };
    let end = start + DOUBLE_FAULT_STACK_SIZE as u64;
    (start..end).contains(&stack_pointer)
}

fn load_gdt_and_segments(
    tables: &DescriptorTables,
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
) {
    // SAFETY: `tables` is backed by static storage and frozen after Phase 3
    // initialization. Its code, data, and TSS descriptors were constructed by
    // the pinned `x86_64` crate immediately before this load.
    unsafe {
        tables.gdt.load_unsafe();
        CS::set_reg(code_selector);
        SS::set_reg(data_selector);
        DS::set_reg(data_selector);
        ES::set_reg(data_selector);
        load_tss(tss_selector);
    }
}
