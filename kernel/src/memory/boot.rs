use core::cell::UnsafeCell;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::println;

pub const MAX_MEMORY_REGIONS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum MemoryKind {
    Usable,
    Reserved,
    AcpiReclaimable,
    AcpiNvs,
    BadMemory,
    BootloaderReclaimable,
    ExecutableAndModules,
    Framebuffer,
    MappedReserved,
    Unknown,
}

impl MemoryKind {
    pub const fn is_allocator_eligible(self) -> bool {
        matches!(self, Self::Usable)
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Usable => "usable",
            Self::Reserved => "reserved",
            Self::AcpiReclaimable => "acpi-reclaimable",
            Self::AcpiNvs => "acpi-nvs",
            Self::BadMemory => "bad-memory",
            Self::BootloaderReclaimable => "bootloader-reclaimable",
            Self::ExecutableAndModules => "executable-and-modules",
            Self::Framebuffer => "framebuffer",
            Self::MappedReserved => "mapped-reserved",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    /// Opaque descriptor value retained for diagnostics only.
    pub source_type: u64,
    pub kind: MemoryKind,
}

impl MemoryRegion {
    const EMPTY: Self = Self {
        start: 0,
        end: 0,
        source_type: 0,
        kind: MemoryKind::Reserved,
    };

    pub const fn len(self) -> u64 {
        self.end - self.start
    }

    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KernelImageInfo {
    pub physical_base: u64,
    pub virtual_base: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FramebufferInfo {
    pub physical_address: u64,
    pub size: u64,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub red_byte: usize,
    pub green_byte: usize,
    pub blue_byte: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RsdpInfo {
    pub physical_address: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootContextError {
    AlreadyCaptured,
    UnsupportedPagingMode,
    TooManyMemoryRegions,
    EmptyMemoryRegion,
    MemoryRegionOverflow,
    InvalidKernelImage,
    InvalidFramebuffer,
    FramebufferNotInHhdm,
    InvalidRsdp,
    RsdpNotInHhdm,
}

impl fmt::Display for BootContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyCaptured => f.write_str("boot context captured twice"),
            Self::UnsupportedPagingMode => {
                f.write_str("bootloader did not enter four-level paging")
            }
            Self::TooManyMemoryRegions => {
                f.write_str("memory map exceeds fixed boot-context capacity")
            }
            Self::EmptyMemoryRegion => f.write_str("memory map contains an empty region"),
            Self::MemoryRegionOverflow => {
                f.write_str("memory map region wraps physical address space")
            }
            Self::InvalidKernelImage => {
                f.write_str("kernel executable address response is invalid")
            }
            Self::InvalidFramebuffer => {
                f.write_str("framebuffer metadata is unsupported or invalid")
            }
            Self::FramebufferNotInHhdm => {
                f.write_str("framebuffer address is outside the boot HHDM")
            }
            Self::InvalidRsdp => f.write_str("ACPI RSDP address is invalid"),
            Self::RsdpNotInHhdm => f.write_str("ACPI RSDP address is outside the boot HHDM"),
        }
    }
}

pub struct BootContext {
    regions: [MemoryRegion; MAX_MEMORY_REGIONS],
    region_count: usize,
    kernel_image: KernelImageInfo,
    framebuffer: Option<FramebufferInfo>,
    rsdp: Option<RsdpInfo>,
}

impl BootContext {
    const EMPTY: Self = Self {
        regions: [MemoryRegion::EMPTY; MAX_MEMORY_REGIONS],
        region_count: 0,
        kernel_image: KernelImageInfo {
            physical_base: 0,
            virtual_base: 0,
        },
        framebuffer: None,
        rsdp: None,
    };

    pub fn memory_regions(&self) -> &[MemoryRegion] {
        &self.regions[..self.region_count]
    }

    pub const fn kernel_image(&self) -> KernelImageInfo {
        self.kernel_image
    }

    pub const fn framebuffer(&self) -> Option<FramebufferInfo> {
        self.framebuffer
    }

    pub const fn rsdp(&self) -> Option<RsdpInfo> {
        self.rsdp
    }

    pub fn dump_memory_map(&self) {
        println!("GAXERA: MEMMAP_BEGIN count={}", self.region_count);
        for (index, region) in self.memory_regions().iter().enumerate() {
            println!(
                "GAXERA: MEMMAP_REGION index={} start={:#018x} end={:#018x} len={:#018x} source_type={} class={} allocator={} reservation=none",
                index,
                region.start,
                region.end,
                region.len(),
                region.source_type,
                region.kind.label(),
                if region.kind.is_allocator_eligible() {
                    "eligible"
                } else {
                    "ineligible"
                },
            );
        }
        println!("GAXERA: MEMMAP_END");
    }

    #[cfg(test)]
    pub(crate) fn for_test(regions: &[MemoryRegion]) -> Self {
        assert!(regions.len() <= MAX_MEMORY_REGIONS);
        let mut context = Self::EMPTY;
        context.kernel_image = KernelImageInfo {
            physical_base: 0x10_0000,
            virtual_base: 0xffff_ffff_8000_0000,
        };
        context.regions[..regions.len()].copy_from_slice(regions);
        context.region_count = regions.len();
        context
    }
}

struct ContextCell(UnsafeCell<BootContext>);

// SAFETY: Phase 4 captures this object exactly once on the bootstrap CPU while
// interrupts are disabled. It is immutable after publication.
unsafe impl Sync for ContextCell {}

static CONTEXT: ContextCell = ContextCell(UnsafeCell::new(BootContext::EMPTY));
static CONTEXT_CAPTURED: AtomicBool = AtomicBool::new(false);

/// Crate-private construction path for the immutable boot boundary.
///
/// Only `arch::x86_64::boot` may translate Limine responses into this
/// Gaxera-owned representation. No Limine type appears in this module's API.
pub(crate) struct BootContextBuilder {
    context: BootContext,
}

impl BootContextBuilder {
    pub(crate) fn new(kernel_image: KernelImageInfo) -> Result<Self, BootContextError> {
        if kernel_image.physical_base == 0 || kernel_image.virtual_base == 0 {
            return Err(BootContextError::InvalidKernelImage);
        }
        Ok(Self {
            context: BootContext {
                kernel_image,
                ..BootContext::EMPTY
            },
        })
    }

    pub(crate) fn push_memory_region(
        &mut self,
        start: u64,
        length: u64,
        raw_type: u64,
        kind: MemoryKind,
    ) -> Result<(), BootContextError> {
        if self.context.region_count == MAX_MEMORY_REGIONS {
            return Err(BootContextError::TooManyMemoryRegions);
        }
        if length == 0 {
            return Err(BootContextError::EmptyMemoryRegion);
        }
        let end = start
            .checked_add(length)
            .ok_or(BootContextError::MemoryRegionOverflow)?;
        self.context.regions[self.context.region_count] = MemoryRegion {
            start,
            end,
            source_type: raw_type,
            kind,
        };
        self.context.region_count += 1;
        Ok(())
    }

    pub(crate) fn set_framebuffer(&mut self, framebuffer: FramebufferInfo) {
        self.context.framebuffer = Some(framebuffer);
    }

    pub(crate) fn set_rsdp(&mut self, rsdp: RsdpInfo) {
        self.context.rsdp = Some(rsdp);
    }

    pub(crate) fn publish(mut self) -> Result<&'static BootContext, BootContextError> {
        sort_regions(&mut self.context.regions[..self.context.region_count]);
        if CONTEXT_CAPTURED.swap(true, Ordering::AcqRel) {
            return Err(BootContextError::AlreadyCaptured);
        }

        // SAFETY: The atomic transition above is the sole publication path.
        // The bootstrap CPU writes the complete value before returning a shared
        // static reference, and no later code obtains mutable access.
        unsafe {
            CONTEXT.0.get().write(self.context);
            Ok(&*CONTEXT.0.get())
        }
    }
}

fn sort_regions(regions: &mut [MemoryRegion]) {
    for index in 1..regions.len() {
        let current = regions[index];
        let mut insertion = index;
        while insertion > 0 && region_key(current) < region_key(regions[insertion - 1]) {
            regions[insertion] = regions[insertion - 1];
            insertion -= 1;
        }
        regions[insertion] = current;
    }
}

fn region_key(region: MemoryRegion) -> (u64, u64, u64) {
    (region.start, region.source_type, region.len())
}

#[cfg(test)]
mod tests {
    use super::{MemoryKind, MemoryRegion, sort_regions};

    #[test]
    fn sorts_regions_by_start_then_type_then_length() {
        let mut regions = [
            MemoryRegion {
                start: 0x3000,
                end: 0x4000,
                source_type: 1,
                kind: MemoryKind::Reserved,
            },
            MemoryRegion {
                start: 0x1000,
                end: 0x3000,
                source_type: 1,
                kind: MemoryKind::Reserved,
            },
            MemoryRegion {
                start: 0x1000,
                end: 0x2000,
                source_type: 0,
                kind: MemoryKind::Usable,
            },
        ];

        sort_regions(&mut regions);

        assert_eq!(regions[0].source_type, 0);
        assert_eq!(regions[1].start, 0x1000);
        assert_eq!(regions[1].end, 0x3000);
        assert_eq!(regions[2].start, 0x3000);
    }

    #[test]
    fn only_usable_memory_is_allocator_eligible() {
        assert!(MemoryKind::Usable.is_allocator_eligible());
        assert!(!MemoryKind::BootloaderReclaimable.is_allocator_eligible());
        assert!(!MemoryKind::Framebuffer.is_allocator_eligible());
    }
}
