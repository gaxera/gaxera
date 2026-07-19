use core::fmt;

use limine::BaseRevision;
use limine::framebuffer::{FRAMEBUFFER_RGB, Framebuffer};
use limine::memmap;
use limine::paging::PagingMode;
use limine::request::{
    EntryPointRequest, ExecutableAddressRequest, FramebufferRequest, HhdmRequest, MemmapRequest,
    ModulesRequest, PagingModeRequest, RsdpRequest,
};

use crate::memory::boot::{
    BootContext, BootContextBuilder, BootContextError, FramebufferInfo, KernelImageInfo,
    MemoryKind, RsdpInfo,
};

unsafe extern "C" {
    fn _start() -> !;
}

#[used]
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static ENTRY_POINT: EntryPointRequest = EntryPointRequest::new(_start);

#[used]
#[unsafe(link_section = ".requests")]
static PAGING_MODE_REQUEST: PagingModeRequest =
    PagingModeRequest::new_exact(PagingMode::X86_64_4LVL);

#[used]
#[unsafe(link_section = ".requests")]
static EXECUTABLE_ADDRESS_REQUEST: ExecutableAddressRequest = ExecutableAddressRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMMAP_REQUEST: MemmapRequest = MemmapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static RSDP_REQUEST: RsdpRequest = RsdpRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MODULES_REQUEST: ModulesRequest = ModulesRequest::new();

pub struct BootHandoff {
    context: &'static BootContext,
    pre_cr3_hhdm_offset: u64,
}

impl BootHandoff {
    pub const fn context(&self) -> &'static BootContext {
        self.context
    }

    /// The Limine direct-map offset, valid only until Gaxera activates CR3.
    pub const fn pre_cr3_hhdm_offset(&self) -> u64 {
        self.pre_cr3_hhdm_offset
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootHandoffError {
    UnsupportedBaseRevision,
    MissingNegotiatedBaseRevision,
    MissingMemoryMap,
    MissingHhdm,
    MissingPagingMode,
    MissingExecutableAddress,
    MissingModules,
    BootContext(BootContextError),
}

impl From<BootContextError> for BootHandoffError {
    fn from(error: BootContextError) -> Self {
        Self::BootContext(error)
    }
}

impl fmt::Display for BootHandoffError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedBaseRevision => f.write_str("unsupported Limine base revision"),
            Self::MissingNegotiatedBaseRevision => {
                f.write_str("Limine did not report a negotiated base revision")
            }
            Self::MissingMemoryMap => f.write_str("Limine memory-map request failed"),
            Self::MissingHhdm => f.write_str("Limine HHDM request failed"),
            Self::MissingPagingMode => f.write_str("Limine paging-mode request failed"),
            Self::MissingExecutableAddress => {
                f.write_str("Limine executable-address request failed")
            }
            Self::MissingModules => f.write_str("Limine module request failed"),
            Self::BootContext(error) => error.fmt(f),
        }
    }
}

/// Convert the Limine handoff into Gaxera-owned data before CR3 changes.
///
/// This is the sole production path that reads Limine response structures.
/// The returned value contains only copied Gaxera metadata and the temporary
/// HHDM offset needed to build inactive page tables.
pub fn capture_handoff() -> Result<BootHandoff, BootHandoffError> {
    if !BASE_REVISION.is_supported() {
        return Err(BootHandoffError::UnsupportedBaseRevision);
    }
    let base_revision = BASE_REVISION
        .actual_revision()
        .ok_or(BootHandoffError::MissingNegotiatedBaseRevision)?;
    let memmap = MEMMAP_REQUEST
        .response()
        .ok_or(BootHandoffError::MissingMemoryMap)?;
    let hhdm = HHDM_REQUEST
        .response()
        .ok_or(BootHandoffError::MissingHhdm)?;
    let paging_mode = PAGING_MODE_REQUEST
        .response()
        .ok_or(BootHandoffError::MissingPagingMode)?;
    let executable = EXECUTABLE_ADDRESS_REQUEST
        .response()
        .ok_or(BootHandoffError::MissingExecutableAddress)?;
    if paging_mode.mode != PagingMode::X86_64_4LVL {
        return Err(BootHandoffError::BootContext(
            BootContextError::UnsupportedPagingMode,
        ));
    }
    let modules = MODULES_REQUEST
        .response()
        .ok_or(BootHandoffError::MissingModules)?;

    let mut context = BootContextBuilder::new(KernelImageInfo {
        physical_base: executable.physical_base,
        virtual_base: executable.virtual_base,
    })
    .map_err(BootHandoffError::BootContext)?;
    for entry in memmap.entries() {
        context
            .push_memory_region(
                entry.base,
                entry.length,
                entry.type_,
                memory_kind(entry.type_),
            )
            .map_err(BootHandoffError::BootContext)?;
    }
    if let Some(framebuffer) = FRAMEBUFFER_REQUEST
        .response()
        .and_then(|response| response.framebuffers().first().copied())
    {
        context.set_framebuffer(framebuffer_info(framebuffer, hhdm.offset)?)
    }
    if let Some(rsdp) = RSDP_REQUEST.response() {
        context.set_rsdp(rsdp_info(rsdp.address, hhdm.offset, base_revision)?);
    }
    for module in modules.modules() {
        let physical_address = (module.data().as_ptr() as u64).saturating_sub(hhdm.offset);
        context.push_boot_module(physical_address, module.data().len() as u64, module.path());
    }

    Ok(BootHandoff {
        context: context.publish().map_err(BootHandoffError::BootContext)?,
        pre_cr3_hhdm_offset: hhdm.offset,
    })
}

const fn memory_kind(raw_type: u64) -> MemoryKind {
    match raw_type {
        memmap::MEMMAP_USABLE => MemoryKind::Usable,
        memmap::MEMMAP_RESERVED => MemoryKind::Reserved,
        memmap::MEMMAP_ACPI_RECLAIMABLE => MemoryKind::AcpiReclaimable,
        memmap::MEMMAP_ACPI_NVS => MemoryKind::AcpiNvs,
        memmap::MEMMAP_BAD_MEMORY => MemoryKind::BadMemory,
        memmap::MEMMAP_BOOTLOADER_RECLAIMABLE => MemoryKind::BootloaderReclaimable,
        memmap::MEMMAP_EXECUTABLE_AND_MODULES => MemoryKind::ExecutableAndModules,
        memmap::MEMMAP_FRAMEBUFFER => MemoryKind::Framebuffer,
        memmap::MEMMAP_MAPPED_RESERVED => MemoryKind::MappedReserved,
        _ => MemoryKind::Unknown,
    }
}

fn framebuffer_info(
    framebuffer: &Framebuffer,
    hhdm_offset: u64,
) -> Result<FramebufferInfo, BootHandoffError> {
    if framebuffer.address().is_null()
        || framebuffer.width == 0
        || framebuffer.height == 0
        || framebuffer.bpp != 32
        || framebuffer.memory_model != FRAMEBUFFER_RGB
        || framebuffer.red_mask_size != 8
        || framebuffer.green_mask_size != 8
        || framebuffer.blue_mask_size != 8
        || !framebuffer.red_mask_shift.is_multiple_of(8)
        || !framebuffer.green_mask_shift.is_multiple_of(8)
        || !framebuffer.blue_mask_shift.is_multiple_of(8)
        || framebuffer.red_mask_shift >= 32
        || framebuffer.green_mask_shift >= 32
        || framebuffer.blue_mask_shift >= 32
    {
        return Err(BootHandoffError::BootContext(
            BootContextError::InvalidFramebuffer,
        ));
    }

    let row_bytes = framebuffer
        .width
        .checked_mul(4)
        .ok_or(BootContextError::InvalidFramebuffer)?;
    let size = framebuffer
        .height
        .checked_mul(framebuffer.pitch)
        .ok_or(BootContextError::InvalidFramebuffer)?;
    if framebuffer.pitch < row_bytes || usize::try_from(size).is_err() {
        return Err(BootHandoffError::BootContext(
            BootContextError::InvalidFramebuffer,
        ));
    }
    let physical_address = (framebuffer.address() as u64)
        .checked_sub(hhdm_offset)
        .ok_or(BootContextError::FramebufferNotInHhdm)?;
    physical_address
        .checked_add(size)
        .ok_or(BootContextError::InvalidFramebuffer)?;

    Ok(FramebufferInfo {
        physical_address,
        size,
        width: framebuffer.width,
        height: framebuffer.height,
        pitch: framebuffer.pitch,
        red_byte: usize::from(framebuffer.red_mask_shift / 8),
        green_byte: usize::from(framebuffer.green_mask_shift / 8),
        blue_byte: usize::from(framebuffer.blue_mask_shift / 8),
    })
}

fn rsdp_info(
    address: *mut (),
    hhdm_offset: u64,
    base_revision: u64,
) -> Result<RsdpInfo, BootHandoffError> {
    if address.is_null() {
        return Err(BootHandoffError::BootContext(BootContextError::InvalidRsdp));
    }
    // Limine defines the RSDP response as physical only for base revision 3.
    // Every other supported base revision exposes a boot HHDM virtual address.
    let physical_address = if base_revision == 3 {
        address as u64
    } else {
        (address as u64)
            .checked_sub(hhdm_offset)
            .ok_or(BootContextError::RsdpNotInHhdm)?
    };
    Ok(RsdpInfo { physical_address })
}
