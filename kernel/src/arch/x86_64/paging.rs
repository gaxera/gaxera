use core::fmt;

use x86_64::registers::control::{Cr0, Cr0Flags, Cr3, Cr3Flags, Cr4, Cr4Flags};
use x86_64::registers::model_specific::{Efer, EferFlags};
use x86_64::structures::paging::mapper::{MappedPageTable, PageTableFrameMapping};
use x86_64::structures::paging::{
    FrameAllocator, Mapper, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, Translate,
};
use x86_64::{PhysAddr, VirtAddr};

use crate::memory::boot::{BootContext, FramebufferInfo, MemoryKind};
use crate::memory::mapping::{
    ACPI_TABLE_WINDOW, FRAMEBUFFER_BASE, HEAP_SIZE, HEAP_START, HHDM_BASE, HHDM_PHYSICAL_LIMIT,
    KERNEL_VIRTUAL_BASE, LOCAL_APIC_WINDOW,
};
use crate::memory::physical::{BootstrapFrameAllocator, PAGE_SIZE, PhysicalAllocatorError};
use crate::println;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PagingError {
    AddressOverflow,
    BootHhdmAddressOverflow,
    HhdmCapacityExceeded,
    KernelVirtualBaseMismatch,
    FrameAllocationFailed,
    PageTableMappingFailed {
        virtual_address: u64,
        physical_address: u64,
    },
    PcidEnabled,
    NoExecuteUnavailable,
    HeapAddressOutsideRegion,
    AcpiTableAddressOutsideReclaimableMemory,
    LocalApicAddressUnaligned,
    LocalApicAddressAliasesRam,
    LocalApicAddressAliasesFramebuffer,
    PageUnmappingFailed {
        virtual_address: u64,
    },
    Physical(PhysicalAllocatorError),
}

impl From<PhysicalAllocatorError> for PagingError {
    fn from(error: PhysicalAllocatorError) -> Self {
        Self::Physical(error)
    }
}

impl fmt::Display for PagingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddressOverflow => f.write_str("virtual or physical address arithmetic overflow"),
            Self::BootHhdmAddressOverflow => {
                f.write_str("a usable physical address overflows the boot HHDM")
            }
            Self::HhdmCapacityExceeded => f.write_str("usable RAM exceeds the Phase 4 HHDM window"),
            Self::KernelVirtualBaseMismatch => {
                f.write_str("kernel image is not at the expected static VMA")
            }
            Self::FrameAllocationFailed => f.write_str("bootstrap frame allocator exhausted"),
            Self::PageTableMappingFailed {
                virtual_address,
                physical_address,
            } => write!(
                f,
                "failed to map virtual {virtual_address:#018x} to physical {physical_address:#018x}"
            ),
            Self::PcidEnabled => f.write_str("PCID is enabled but Phase 4 does not support it"),
            Self::NoExecuteUnavailable => f.write_str("NXE could not be enabled"),
            Self::HeapAddressOutsideRegion => {
                f.write_str("virtual address is outside the fixed Phase 4 heap")
            }
            Self::AcpiTableAddressOutsideReclaimableMemory => {
                f.write_str("ACPI table address is outside ACPI reclaimable memory")
            }
            Self::LocalApicAddressUnaligned => {
                f.write_str("Local APIC physical address is not page aligned")
            }
            Self::LocalApicAddressAliasesRam => {
                f.write_str("Local APIC physical address aliases Gaxera's RAM-only HHDM")
            }
            Self::LocalApicAddressAliasesFramebuffer => {
                f.write_str("Local APIC physical address aliases the framebuffer")
            }
            Self::PageUnmappingFailed { virtual_address } => {
                write!(f, "failed to unmap virtual page {virtual_address:#018x}")
            }
            Self::Physical(error) => error.fmt(f),
        }
    }
}

#[derive(Clone, Copy)]
struct FrameMapping {
    offset: u64,
}

// SAFETY: Every page-table frame passed to this mapper is allocated from a
// usable region that the active direct map covers at `offset + physical`.
unsafe impl PageTableFrameMapping for FrameMapping {
    fn frame_to_pointer(&self, frame: PhysFrame) -> *mut PageTable {
        (self.offset + frame.start_address().as_u64()) as *mut PageTable
    }
}

pub struct KernelPageTables {
    root: PhysFrame,
}

pub fn framebuffer_virtual_address(framebuffer: FramebufferInfo) -> Result<u64, PagingError> {
    let page_offset = framebuffer.physical_address - align_down(framebuffer.physical_address);
    FRAMEBUFFER_BASE
        .checked_add(page_offset)
        .ok_or(PagingError::AddressOverflow)
}

impl KernelPageTables {
    /// Build Gaxera's inactive four-level page tables while Limine's HHDM is
    /// still active. The resulting hierarchy has no dependency on Limine
    /// tables; it maps only Gaxera-selected ranges.
    ///
    /// # Safety
    /// `boot_hhdm_offset` must map every usable frame returned by `allocator`
    /// until CR3 is changed. Interrupts must stay disabled throughout setup.
    pub unsafe fn build(
        context: &BootContext,
        boot_hhdm_offset: u64,
        allocator: &mut BootstrapFrameAllocator,
    ) -> Result<Self, PagingError> {
        if context.kernel_image().virtual_base != KERNEL_VIRTUAL_BASE {
            return Err(PagingError::KernelVirtualBaseMismatch);
        }
        validate_boot_hhdm(context, boot_hhdm_offset)?;

        let root = unsafe { allocator.allocate_zeroed(boot_hhdm_offset) }?
            .ok_or(PagingError::FrameAllocationFailed)?;
        let root_address = boot_hhdm_offset
            .checked_add(root.start_address().as_u64())
            .ok_or(PagingError::AddressOverflow)?;
        // SAFETY: `root` is a unique, zeroed 4 KiB frame reachable through the
        // Limine HHDM. It is the root of the hierarchy built below.
        let root_table = unsafe { &mut *(root_address as *mut PageTable) };
        let mapping = FrameMapping {
            offset: boot_hhdm_offset,
        };
        // SAFETY: every descendant table frame will be supplied by `allocator`
        // from Limine-HHDM-mapped usable RAM; `root_table` is valid and zeroed.
        let mut mapper = unsafe { MappedPageTable::new(root_table, mapping) };

        for region in context.memory_regions() {
            if !region.kind.is_allocator_eligible() {
                continue;
            }
            let start = align_up(region.start)?;
            let end = align_down(region.end);
            if start >= end {
                continue;
            }
            if end > HHDM_PHYSICAL_LIMIT {
                return Err(PagingError::HhdmCapacityExceeded);
            }
            map_range(
                &mut mapper,
                allocator,
                HHDM_BASE
                    .checked_add(start)
                    .ok_or(PagingError::AddressOverflow)?,
                start,
                end,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE,
            )?;
        }
        println!("GAXERA: PAGING_HHDM_MAPPED");

        let image = context.kernel_image();
        map_kernel_section(
            &mut mapper,
            allocator,
            image.physical_base,
            image.virtual_base,
            linker_symbol(&raw const __text_start),
            linker_symbol(&raw const __text_end),
            PageTableFlags::PRESENT,
        )?;
        println!("GAXERA: PAGING_TEXT_MAPPED");
        map_kernel_section(
            &mut mapper,
            allocator,
            image.physical_base,
            image.virtual_base,
            linker_symbol(&raw const __rodata_start),
            linker_symbol(&raw const __rodata_end),
            PageTableFlags::PRESENT | PageTableFlags::NO_EXECUTE,
        )?;
        map_kernel_section(
            &mut mapper,
            allocator,
            image.physical_base,
            image.virtual_base,
            linker_symbol(&raw const __requests_start),
            linker_symbol(&raw const __requests_end),
            PageTableFlags::PRESENT | PageTableFlags::NO_EXECUTE,
        )?;
        for (start, end) in [
            (
                linker_symbol(&raw const __data_start),
                linker_symbol(&raw const __bss_end),
            ),
            (
                linker_symbol(&raw const __bootstrap_stack_start),
                linker_symbol(&raw const __bootstrap_stack_end),
            ),
            (
                linker_symbol(&raw const __ist_stack_start),
                linker_symbol(&raw const __ist_stack_end),
            ),
            (
                linker_symbol(&raw const __user_transition_stack_start),
                linker_symbol(&raw const __user_transition_stack_end),
            ),
        ] {
            map_kernel_section(
                &mut mapper,
                allocator,
                image.physical_base,
                image.virtual_base,
                start,
                end,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE,
            )?;
        }

        if let Some(framebuffer) = context.framebuffer() {
            map_framebuffer(&mut mapper, allocator, framebuffer)?;
        }
        println!("GAXERA: PAGING_FRAMEBUFFER_MAPPED");

        Ok(Self { root })
    }

    pub const fn root_frame(&self) -> PhysFrame {
        self.root
    }

    /// Activate the page-table hierarchy after the caller has completed the
    /// continuity audit. CR3 reload flushes non-global TLB translations.
    ///
    /// # Safety
    /// The hierarchy must map the executing code, current stack, descriptor
    /// state, and all memory touched immediately after this instruction.
    pub unsafe fn activate(&self) -> Result<(), PagingError> {
        if Cr4::read().contains(Cr4Flags::PCID) {
            return Err(PagingError::PcidEnabled);
        }
        // SAFETY: CR0 paging remains enabled; this only adds ring-0 write
        // protection so read-only kernel mappings are actually enforced.
        unsafe { Cr0::update(|flags| flags.insert(Cr0Flags::WRITE_PROTECT)) };
        // SAFETY: x86-64 QEMU/OVMF supports NXE. The flag is verified below
        // before any NX mapping becomes active.
        unsafe { Efer::update(|flags| flags.insert(EferFlags::NO_EXECUTE_ENABLE)) };
        if !Efer::read().contains(EferFlags::NO_EXECUTE_ENABLE) {
            return Err(PagingError::NoExecuteUnavailable);
        }
        // SAFETY: caller proved the complete transition continuity set.
        unsafe { Cr3::write(self.root, Cr3Flags::empty()) };
        Ok(())
    }

    /// # Safety
    /// Gaxera's RAM-only HHDM must be active and must map all physical frames
    /// used as page tables by the supplied allocator.
    pub unsafe fn map_heap_page<A>(
        &mut self,
        virtual_address: u64,
        frame: PhysFrame,
        allocator: &mut A,
    ) -> Result<(), PagingError>
    where
        A: FrameAllocator<Size4KiB>,
    {
        let heap_end = HEAP_START
            .checked_add(HEAP_SIZE)
            .ok_or(PagingError::AddressOverflow)?;
        if virtual_address < HEAP_START
            || virtual_address >= heap_end
            || !virtual_address.is_multiple_of(PAGE_SIZE)
        {
            return Err(PagingError::HeapAddressOutsideRegion);
        }
        // SAFETY: this checked wrapper restricts the mapping to an unmapped,
        // page-aligned heap page and fixes its permissions to RW+NX. Its caller
        // supplies the remaining HHDM and allocator exclusivity invariants.
        unsafe {
            self.map_page(
                virtual_address,
                frame,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE,
                allocator,
            )
        }
    }

    /// Map one ACPI-reclaimable page into the fixed temporary firmware window.
    ///
    /// # Safety
    /// Interrupts must be disabled and the caller must unmap this window before
    /// mapping another physical page. `allocator` must satisfy the active HHDM
    /// frame-mapping invariant documented by `map_page`.
    pub(crate) unsafe fn map_acpi_table_page<A>(
        &mut self,
        context: &BootContext,
        physical_address: u64,
        allocator: &mut A,
    ) -> Result<(), PagingError>
    where
        A: FrameAllocator<Size4KiB>,
    {
        if !physical_address.is_multiple_of(PAGE_SIZE)
            || !physical_range_has_kind(
                context,
                physical_address,
                PAGE_SIZE,
                MemoryKind::AcpiReclaimable,
            )
        {
            return Err(PagingError::AcpiTableAddressOutsideReclaimableMemory);
        }
        let frame = frame_from_physical(physical_address)?;
        // SAFETY: this maps the single fixed firmware window read-only and NX.
        // The caller proves the window is currently unmapped and exclusively used.
        unsafe {
            self.map_page(
                ACPI_TABLE_WINDOW,
                frame,
                PageTableFlags::PRESENT | PageTableFlags::NO_EXECUTE,
                allocator,
            )
        }
    }

    /// Remove the current temporary firmware-table mapping and flush its TLB entry.
    ///
    /// # Safety
    /// The caller must ensure no reference derived from `ACPI_TABLE_WINDOW`
    /// remains live. Interrupts must stay disabled throughout the operation.
    pub(crate) unsafe fn unmap_acpi_table_page(&mut self) -> Result<(), PagingError> {
        // SAFETY: the active HHDM maps `root`, and the sole temporary window is
        // not concurrently accessed while this mapper mutably borrows the hierarchy.
        unsafe { self.unmap_page(ACPI_TABLE_WINDOW) }
    }

    /// Permanently map the validated Local APIC page at Gaxera's dedicated UC window.
    ///
    /// # Safety
    /// The caller must have validated the CPU's PAT state and xAPIC mode before
    /// relying on the PWT+PCD UC cache selection. Interrupts must be disabled;
    /// this is the only Gaxera mapping of the Local APIC physical page.
    pub(crate) unsafe fn map_local_apic_page<A>(
        &mut self,
        context: &BootContext,
        physical_address: u64,
        allocator: &mut A,
    ) -> Result<(), PagingError>
    where
        A: FrameAllocator<Size4KiB>,
    {
        if !physical_address.is_multiple_of(PAGE_SIZE) {
            return Err(PagingError::LocalApicAddressUnaligned);
        }
        if physical_range_has_kind(context, physical_address, PAGE_SIZE, MemoryKind::Usable) {
            return Err(PagingError::LocalApicAddressAliasesRam);
        }
        if framebuffer_overlaps(context.framebuffer(), physical_address, PAGE_SIZE)? {
            return Err(PagingError::LocalApicAddressAliasesFramebuffer);
        }
        let frame = frame_from_physical(physical_address)?;
        // SAFETY: the caller proved the one-mapping UC policy. PWT+PCD select
        // PAT entry 3 for a 4 KiB page; APIC initialization validates that entry.
        unsafe {
            self.map_page(
                LOCAL_APIC_WINDOW,
                frame,
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::NO_EXECUTE
                    | PageTableFlags::WRITE_THROUGH
                    | PageTableFlags::NO_CACHE,
                allocator,
            )
        }
    }

    /// # Safety
    /// Gaxera's RAM-only HHDM must be active and must map all physical frames
    /// used as page tables by the supplied allocator. The caller must also
    /// prove that the destination is currently unmapped and that its flags
    /// satisfy a reviewed mapping policy.
    unsafe fn map_page<A>(
        &mut self,
        virtual_address: u64,
        frame: PhysFrame,
        flags: PageTableFlags,
        allocator: &mut A,
    ) -> Result<(), PagingError>
    where
        A: FrameAllocator<Size4KiB>,
    {
        let root_address = HHDM_BASE
            .checked_add(self.root.start_address().as_u64())
            .ok_or(PagingError::AddressOverflow)?;
        // SAFETY: the active HHDM maps `root`, and `FrameMapping` is valid for
        // every frame allocated from usable RAM by `allocator`.
        let root = unsafe { &mut *(root_address as *mut PageTable) };
        let mapping = FrameMapping { offset: HHDM_BASE };
        let mut mapper = unsafe { MappedPageTable::new(root, mapping) };
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virtual_address));
        // SAFETY: caller owns `frame`, the virtual page is not currently
        // mapped, and the requested flags conform to the Phase 4 policy.
        let flush = unsafe { mapper.map_to(page, frame, flags, allocator) }.map_err(|_| {
            PagingError::PageTableMappingFailed {
                virtual_address,
                physical_address: frame.start_address().as_u64(),
            }
        })?;
        flush.flush();
        Ok(())
    }

    /// # Safety
    /// The active HHDM must map `root`, the page must be mapped, and no code may
    /// retain or access pointers into the virtual page after this operation.
    unsafe fn unmap_page(&mut self, virtual_address: u64) -> Result<(), PagingError> {
        let root_address = HHDM_BASE
            .checked_add(self.root.start_address().as_u64())
            .ok_or(PagingError::AddressOverflow)?;
        // SAFETY: caller excludes concurrent table mutation and pointer use;
        // the active HHDM maps the root table frame.
        let root = unsafe { &mut *(root_address as *mut PageTable) };
        let mut mapper = unsafe { MappedPageTable::new(root, FrameMapping { offset: HHDM_BASE }) };
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virtual_address));
        let (_, flush) = mapper
            .unmap(page)
            .map_err(|_| PagingError::PageUnmappingFailed { virtual_address })?;
        flush.flush();
        Ok(())
    }

    /// Translate through Gaxera's active root table.
    ///
    /// # Safety
    /// The caller must exclude concurrent mutation of this hierarchy. The
    /// `x86_64` mapper API requires a mutable root-table reference even for
    /// translation; this method does not mutate page-table entries itself.
    pub unsafe fn translate(&self, virtual_address: u64) -> Option<PhysAddr> {
        let root_address = HHDM_BASE.checked_add(self.root.start_address().as_u64())?;
        // SAFETY: the active HHDM maps the root; the caller excludes all
        // aliases which could mutate the hierarchy while this mapper exists.
        let root = unsafe { &mut *(root_address as *mut PageTable) };
        let mapper = unsafe { MappedPageTable::new(root, FrameMapping { offset: HHDM_BASE }) };
        mapper.translate_addr(VirtAddr::new(virtual_address))
    }
}

fn validate_boot_hhdm(context: &BootContext, boot_hhdm_offset: u64) -> Result<(), PagingError> {
    for region in context.memory_regions() {
        if !region.kind.is_allocator_eligible() || region.is_empty() {
            continue;
        }
        boot_hhdm_offset
            .checked_add(region.end - 1)
            .ok_or(PagingError::BootHhdmAddressOverflow)?;
    }
    Ok(())
}

fn frame_from_physical(physical_address: u64) -> Result<PhysFrame, PagingError> {
    PhysFrame::from_start_address(
        PhysAddr::try_new(physical_address).map_err(|_| PagingError::AddressOverflow)?,
    )
    .map_err(|_| PagingError::AddressOverflow)
}

fn physical_range_has_kind(
    context: &BootContext,
    physical_address: u64,
    length: u64,
    expected_kind: MemoryKind,
) -> bool {
    let Some(end) = physical_address.checked_add(length) else {
        return false;
    };
    context.memory_regions().iter().any(|region| {
        region.kind == expected_kind && physical_address >= region.start && end <= region.end
    })
}

fn framebuffer_overlaps(
    framebuffer: Option<FramebufferInfo>,
    physical_address: u64,
    length: u64,
) -> Result<bool, PagingError> {
    let Some(framebuffer) = framebuffer else {
        return Ok(false);
    };
    let end = physical_address
        .checked_add(length)
        .ok_or(PagingError::AddressOverflow)?;
    let framebuffer_end = framebuffer
        .physical_address
        .checked_add(framebuffer.size)
        .ok_or(PagingError::AddressOverflow)?;
    Ok(physical_address < framebuffer_end && framebuffer.physical_address < end)
}

unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __requests_start: u8;
    static __requests_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
    static __bootstrap_stack_start: u8;
    static __bootstrap_stack_end: u8;
    static __ist_stack_start: u8;
    static __ist_stack_end: u8;
    static __user_transition_stack_start: u8;
    static __user_transition_stack_end: u8;
}

fn linker_symbol(symbol: *const u8) -> u64 {
    symbol as u64
}

fn map_kernel_section(
    mapper: &mut MappedPageTable<'_, FrameMapping>,
    allocator: &mut BootstrapFrameAllocator,
    physical_base: u64,
    virtual_base: u64,
    start: u64,
    end: u64,
    flags: PageTableFlags,
) -> Result<(), PagingError> {
    if start >= end {
        return Ok(());
    }
    let start = align_down(start);
    let end = align_up(end)?;
    let physical = physical_base
        .checked_add(
            start
                .checked_sub(virtual_base)
                .ok_or(PagingError::AddressOverflow)?,
        )
        .ok_or(PagingError::AddressOverflow)?;
    map_range(
        mapper,
        allocator,
        start,
        physical,
        physical
            .checked_add(end - start)
            .ok_or(PagingError::AddressOverflow)?,
        flags,
    )
}

fn map_framebuffer(
    mapper: &mut MappedPageTable<'_, FrameMapping>,
    allocator: &mut BootstrapFrameAllocator,
    framebuffer: FramebufferInfo,
) -> Result<(), PagingError> {
    let physical_start = align_down(framebuffer.physical_address);
    let physical_end = align_up(
        framebuffer
            .physical_address
            .checked_add(framebuffer.size)
            .ok_or(PagingError::AddressOverflow)?,
    )?;
    map_range(
        mapper,
        allocator,
        FRAMEBUFFER_BASE,
        physical_start,
        physical_end,
        PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::NO_EXECUTE
            | PageTableFlags::NO_CACHE,
    )
}

fn map_range(
    mapper: &mut MappedPageTable<'_, FrameMapping>,
    allocator: &mut BootstrapFrameAllocator,
    virtual_start: u64,
    physical_start: u64,
    physical_end: u64,
    flags: PageTableFlags,
) -> Result<(), PagingError> {
    let mut virtual_address = virtual_start;
    let mut physical_address = physical_start;
    while physical_address < physical_end {
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virtual_address));
        let frame = PhysFrame::from_start_address(
            PhysAddr::try_new(physical_address).map_err(|_| PagingError::AddressOverflow)?,
        )
        .map_err(|_| PagingError::AddressOverflow)?;
        // SAFETY: this builds an inactive hierarchy; the allocator returns
        // unique frame-table frames and every requested mapping is page-aligned.
        unsafe { mapper.map_to(page, frame, flags, allocator) }
            .map_err(|_| PagingError::PageTableMappingFailed {
                virtual_address,
                physical_address,
            })?
            .ignore();
        virtual_address = virtual_address
            .checked_add(PAGE_SIZE)
            .ok_or(PagingError::AddressOverflow)?;
        physical_address = physical_address
            .checked_add(PAGE_SIZE)
            .ok_or(PagingError::AddressOverflow)?;
    }
    Ok(())
}

fn align_up(address: u64) -> Result<u64, PagingError> {
    address
        .checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
        .ok_or(PagingError::AddressOverflow)
}

const fn align_down(address: u64) -> u64 {
    address & !(PAGE_SIZE - 1)
}
