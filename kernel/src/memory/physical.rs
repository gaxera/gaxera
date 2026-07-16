use core::cell::UnsafeCell;
use core::fmt;
use core::mem::MaybeUninit;
use core::ptr;
use core::slice;
use core::sync::atomic::{AtomicBool, Ordering};

use x86_64::PhysAddr;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator, PageSize, PhysFrame, Size4KiB};

use crate::memory::boot::{BootContext, MemoryKind};

pub const PAGE_SIZE: u64 = Size4KiB::SIZE;
const MAX_USABLE_RANGES: usize = 64;
const MAX_BOOT_RESERVATIONS: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PhysicalRange {
    pub start: u64,
    pub end: u64,
}

impl PhysicalRange {
    const EMPTY: Self = Self { start: 0, end: 0 };

    pub const fn frame_count(self) -> u64 {
        (self.end - self.start) / PAGE_SIZE
    }

    pub const fn contains_frame(self, frame: u64) -> bool {
        frame >= self.start && frame < self.end && frame.is_multiple_of(PAGE_SIZE)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhysicalAllocatorError {
    AddressOverflow,
    InvalidPhysicalAddress,
    TooManyUsableRanges,
    TooManyReservations,
    OverlappingUsableRanges,
    InvalidReservation,
    InsufficientBitmapStorage,
    FrameOutsideManagedMemory,
    FrameAlreadyFree,
    GlobalAllocatorAlreadyInitialized,
}

impl fmt::Display for PhysicalAllocatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AddressOverflow => f.write_str("physical address arithmetic overflow"),
            Self::InvalidPhysicalAddress => {
                f.write_str("physical address is unsupported by x86-64")
            }
            Self::TooManyUsableRanges => {
                f.write_str("usable memory exceeds fixed allocator range capacity")
            }
            Self::TooManyReservations => f.write_str("boot reservation capacity exhausted"),
            Self::OverlappingUsableRanges => f.write_str("usable memory map ranges overlap"),
            Self::InvalidReservation => {
                f.write_str("reservation is not a non-empty page-aligned range")
            }
            Self::InsufficientBitmapStorage => f.write_str("bitmap backing is too small"),
            Self::FrameOutsideManagedMemory => {
                f.write_str("frame is outside managed usable memory")
            }
            Self::FrameAlreadyFree => f.write_str("frame is already free"),
            Self::GlobalAllocatorAlreadyInitialized => {
                f.write_str("segmented physical allocator initialized twice")
            }
        }
    }
}

pub struct BootReservations {
    ranges: [PhysicalRange; MAX_BOOT_RESERVATIONS],
    count: usize,
}

impl BootReservations {
    pub const fn new() -> Self {
        Self {
            ranges: [PhysicalRange::EMPTY; MAX_BOOT_RESERVATIONS],
            count: 0,
        }
    }

    pub fn ranges(&self) -> &[PhysicalRange] {
        &self.ranges[..self.count]
    }

    pub fn reserve_frame(&mut self, frame: PhysFrame) -> Result<(), PhysicalAllocatorError> {
        let start = frame.start_address().as_u64();
        let end = start
            .checked_add(PAGE_SIZE)
            .ok_or(PhysicalAllocatorError::AddressOverflow)?;
        self.reserve_range(PhysicalRange { start, end })
    }

    pub fn reserve_range(&mut self, range: PhysicalRange) -> Result<(), PhysicalAllocatorError> {
        if range.start >= range.end
            || !range.start.is_multiple_of(PAGE_SIZE)
            || !range.end.is_multiple_of(PAGE_SIZE)
        {
            return Err(PhysicalAllocatorError::InvalidReservation);
        }
        if self.count > 0 {
            let last = &mut self.ranges[self.count - 1];
            if range.start <= last.end && range.end >= last.start {
                last.start = last.start.min(range.start);
                last.end = last.end.max(range.end);
                return Ok(());
            }
        }
        if self.count == MAX_BOOT_RESERVATIONS {
            return Err(PhysicalAllocatorError::TooManyReservations);
        }
        self.ranges[self.count] = range;
        self.count += 1;
        Ok(())
    }

    pub fn contains_frame(&self, frame: u64) -> bool {
        self.ranges()
            .iter()
            .any(|range| range.contains_frame(frame))
    }
}

impl Default for BootReservations {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BootstrapFrameAllocator {
    ranges: [PhysicalRange; MAX_USABLE_RANGES],
    next: [u64; MAX_USABLE_RANGES],
    range_count: usize,
    current_range: usize,
    reservations: BootReservations,
}

impl BootstrapFrameAllocator {
    pub fn from_boot_context(context: &BootContext) -> Result<Self, PhysicalAllocatorError> {
        let mut allocator = Self {
            ranges: [PhysicalRange::EMPTY; MAX_USABLE_RANGES],
            next: [0; MAX_USABLE_RANGES],
            range_count: 0,
            current_range: 0,
            reservations: BootReservations::new(),
        };

        for region in context.memory_regions() {
            if region.kind != MemoryKind::Usable {
                continue;
            }
            let start = align_up(region.start)?;
            let end = align_down(region.end);
            if start >= end {
                continue;
            }
            if allocator.range_count == MAX_USABLE_RANGES {
                return Err(PhysicalAllocatorError::TooManyUsableRanges);
            }
            if allocator.range_count > 0 {
                let previous = allocator.ranges[allocator.range_count - 1];
                if start < previous.end {
                    return Err(PhysicalAllocatorError::OverlappingUsableRanges);
                }
            }
            allocator.ranges[allocator.range_count] = PhysicalRange { start, end };
            allocator.next[allocator.range_count] = start;
            allocator.range_count += 1;
        }

        Ok(allocator)
    }

    pub fn reservations(&self) -> &BootReservations {
        &self.reservations
    }

    pub fn usable_ranges(&self) -> &[PhysicalRange] {
        &self.ranges[..self.range_count]
    }

    pub fn allocate(&mut self) -> Result<Option<PhysFrame>, PhysicalAllocatorError> {
        while self.current_range < self.range_count {
            let range = self.ranges[self.current_range];
            let next = self.next[self.current_range];
            if next == range.end {
                self.current_range += 1;
                continue;
            }

            let frame = frame_from_address(next)?;
            self.next[self.current_range] = next
                .checked_add(PAGE_SIZE)
                .ok_or(PhysicalAllocatorError::AddressOverflow)?;
            self.reservations.reserve_frame(frame)?;
            return Ok(Some(frame));
        }
        Ok(None)
    }

    pub fn allocate_contiguous(
        &mut self,
        frame_count: u64,
    ) -> Result<Option<PhysicalRange>, PhysicalAllocatorError> {
        if frame_count == 0 {
            return Err(PhysicalAllocatorError::InvalidReservation);
        }
        let byte_count = frame_count
            .checked_mul(PAGE_SIZE)
            .ok_or(PhysicalAllocatorError::AddressOverflow)?;
        while self.current_range < self.range_count {
            let range = self.ranges[self.current_range];
            let start = self.next[self.current_range];
            let Some(end) = start.checked_add(byte_count) else {
                return Err(PhysicalAllocatorError::AddressOverflow);
            };
            if end > range.end {
                self.current_range += 1;
                continue;
            }
            let allocation = PhysicalRange { start, end };
            self.next[self.current_range] = end;
            self.reservations.reserve_range(allocation)?;
            return Ok(Some(allocation));
        }
        Ok(None)
    }

    /// Allocate and zero a page-table-ready frame through the Limine HHDM.
    ///
    /// # Safety
    /// `hhdm_offset + frame` must be a valid writable mapping for every frame
    /// returned by this allocator. Limine provides that for usable memory until
    /// Gaxera activates its own CR3.
    pub unsafe fn allocate_zeroed(
        &mut self,
        hhdm_offset: u64,
    ) -> Result<Option<PhysFrame>, PhysicalAllocatorError> {
        let Some(frame) = self.allocate()? else {
            return Ok(None);
        };
        let virtual_address = hhdm_offset
            .checked_add(frame.start_address().as_u64())
            .ok_or(PhysicalAllocatorError::AddressOverflow)?;
        // SAFETY: caller guarantees that this HHDM address maps one writable
        // frame. The allocation is unique and has not been exposed elsewhere.
        unsafe { ptr::write_bytes(virtual_address as *mut u8, 0, PAGE_SIZE as usize) };
        Ok(Some(frame))
    }
}

// SAFETY: `BootstrapFrameAllocator` advances a private cursor through
// non-overlapping usable ranges and records each returned frame immediately.
unsafe impl FrameAllocator<Size4KiB> for BootstrapFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        self.allocate().ok().flatten()
    }
}

#[derive(Clone, Copy)]
struct ManagedRange {
    physical: PhysicalRange,
    bitmap_start: u64,
}

impl ManagedRange {
    const EMPTY: Self = Self {
        physical: PhysicalRange::EMPTY,
        bitmap_start: 0,
    };
}

pub struct SegmentedBitmapFrameAllocator<'a> {
    bitmap: &'a mut [u64],
    ranges: [ManagedRange; MAX_USABLE_RANGES],
    range_count: usize,
    frame_count: u64,
}

struct AllocatorCell(UnsafeCell<MaybeUninit<SegmentedBitmapFrameAllocator<'static>>>);

// SAFETY: Phase 4 initializes and accesses the allocator while interrupts are
// disabled on its sole bootstrap CPU. Later SMP synchronization is explicitly
// outside this phase and must replace this access discipline before reuse.
unsafe impl Sync for AllocatorCell {}

static GLOBAL_ALLOCATOR: AllocatorCell = AllocatorCell(UnsafeCell::new(MaybeUninit::uninit()));
static GLOBAL_ALLOCATOR_INITIALIZED: AtomicBool = AtomicBool::new(false);

impl<'a> SegmentedBitmapFrameAllocator<'a> {
    pub fn required_words(context: &BootContext) -> Result<usize, PhysicalAllocatorError> {
        let ranges = normalized_usable_ranges(context)?;
        let mut frames = 0_u64;
        for range in ranges.iter().flatten() {
            frames = frames
                .checked_add(range.frame_count())
                .ok_or(PhysicalAllocatorError::AddressOverflow)?;
        }
        usize::try_from(frames.div_ceil(64)).map_err(|_| PhysicalAllocatorError::AddressOverflow)
    }

    pub fn new(
        context: &BootContext,
        reservations: &BootReservations,
        bitmap: &'a mut [u64],
    ) -> Result<Self, PhysicalAllocatorError> {
        let normalized = normalized_usable_ranges(context)?;
        let mut allocator = Self {
            bitmap,
            ranges: [ManagedRange::EMPTY; MAX_USABLE_RANGES],
            range_count: 0,
            frame_count: 0,
        };
        for word in allocator.bitmap.iter_mut() {
            *word = u64::MAX;
        }

        for range in normalized.iter().flatten() {
            let start_index = allocator.frame_count;
            allocator.ranges[allocator.range_count] = ManagedRange {
                physical: *range,
                bitmap_start: start_index,
            };
            allocator.range_count += 1;
            allocator.frame_count = allocator
                .frame_count
                .checked_add(range.frame_count())
                .ok_or(PhysicalAllocatorError::AddressOverflow)?;
        }
        let required_words = usize::try_from(allocator.frame_count.div_ceil(64))
            .map_err(|_| PhysicalAllocatorError::AddressOverflow)?;
        if allocator.bitmap.len() < required_words {
            return Err(PhysicalAllocatorError::InsufficientBitmapStorage);
        }

        for index in 0..allocator.frame_count {
            let frame_address = allocator.frame_address(index)?;
            if !reservations.contains_frame(frame_address) {
                allocator.mark_free(index);
            }
        }
        Ok(allocator)
    }

    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    pub fn allocate(&mut self) -> Option<PhysFrame> {
        for word_index in 0..self.bitmap.len() {
            let free_bits = !self.bitmap[word_index];
            if free_bits == 0 {
                continue;
            }
            let bit = u64::from(free_bits.trailing_zeros());
            let index = (word_index as u64) * 64 + bit;
            if index >= self.frame_count {
                return None;
            }
            self.mark_used(index);
            return self
                .frame_address(index)
                .ok()
                .and_then(|address| frame_from_address(address).ok());
        }
        None
    }

    /// # Safety
    /// The caller must ensure this frame is no longer mapped, referenced, or
    /// used as allocator metadata or a page table.
    pub unsafe fn deallocate(&mut self, frame: PhysFrame) -> Result<(), PhysicalAllocatorError> {
        let index = self.frame_index(frame.start_address().as_u64())?;
        if !self.is_used(index) {
            return Err(PhysicalAllocatorError::FrameAlreadyFree);
        }
        self.mark_free(index);
        Ok(())
    }

    fn frame_address(&self, index: u64) -> Result<u64, PhysicalAllocatorError> {
        for range in &self.ranges[..self.range_count] {
            let range_end = range.bitmap_start + range.physical.frame_count();
            if index < range_end {
                return range
                    .physical
                    .start
                    .checked_add((index - range.bitmap_start) * PAGE_SIZE)
                    .ok_or(PhysicalAllocatorError::AddressOverflow);
            }
        }
        Err(PhysicalAllocatorError::FrameOutsideManagedMemory)
    }

    fn frame_index(&self, address: u64) -> Result<u64, PhysicalAllocatorError> {
        for range in &self.ranges[..self.range_count] {
            if range.physical.contains_frame(address) {
                return Ok(range.bitmap_start + (address - range.physical.start) / PAGE_SIZE);
            }
        }
        Err(PhysicalAllocatorError::FrameOutsideManagedMemory)
    }

    fn is_used(&self, index: u64) -> bool {
        let word = (index / 64) as usize;
        let bit = index % 64;
        self.bitmap[word] & (1_u64 << bit) != 0
    }

    fn mark_used(&mut self, index: u64) {
        let word = (index / 64) as usize;
        let bit = index % 64;
        self.bitmap[word] |= 1_u64 << bit;
    }

    fn mark_free(&mut self, index: u64) {
        let word = (index / 64) as usize;
        let bit = index % 64;
        self.bitmap[word] &= !(1_u64 << bit);
    }
}

/// Install the segmented physical allocator in permanent static storage.
///
/// # Safety
/// `bitmap` must name `bitmap_words` writable `u64`s in frames permanently
/// reserved from the managed physical-memory set. Initialization is one-time,
/// interrupts must remain disabled, and callers must uphold exclusive mutable
/// access until a synchronized allocator interface exists.
pub unsafe fn initialize_global_allocator(
    context: &BootContext,
    reservations: &BootReservations,
    bitmap: *mut u64,
    bitmap_words: usize,
) -> Result<&'static mut SegmentedBitmapFrameAllocator<'static>, PhysicalAllocatorError> {
    if GLOBAL_ALLOCATOR_INITIALIZED.swap(true, Ordering::SeqCst) {
        return Err(PhysicalAllocatorError::GlobalAllocatorAlreadyInitialized);
    }

    // SAFETY: caller reserves the backing frames for the kernel lifetime and
    // transfers exclusive ownership of exactly this bitmap range.
    let bitmap = unsafe { slice::from_raw_parts_mut(bitmap, bitmap_words) };
    let allocator = SegmentedBitmapFrameAllocator::new(context, reservations, bitmap)?;
    // SAFETY: one-time atomic gate above makes this the sole initialization of
    // static storage. The stored allocator outlives all Phase 4 consumers.
    unsafe { (*GLOBAL_ALLOCATOR.0.get()).write(allocator) };
    // SAFETY: storage was initialized directly above and will never move.
    Ok(unsafe { (&mut *GLOBAL_ALLOCATOR.0.get()).assume_init_mut() })
}

// SAFETY: The bitmap marks a frame used before returning it, and only its
// mutable owner can allocate. Constructor reserves all bootstrap allocations.
unsafe impl FrameAllocator<Size4KiB> for SegmentedBitmapFrameAllocator<'_> {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        self.allocate()
    }
}

impl FrameDeallocator<Size4KiB> for SegmentedBitmapFrameAllocator<'_> {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame) {
        // SAFETY: FrameDeallocator transfers the same unused-frame obligation
        // documented on `deallocate` to its caller.
        let _ = unsafe { self.deallocate(frame) };
    }
}

fn normalized_usable_ranges(
    context: &BootContext,
) -> Result<[Option<PhysicalRange>; MAX_USABLE_RANGES], PhysicalAllocatorError> {
    let mut ranges: [Option<PhysicalRange>; MAX_USABLE_RANGES] = [None; MAX_USABLE_RANGES];
    let mut count = 0;
    for region in context.memory_regions() {
        if region.kind != MemoryKind::Usable {
            continue;
        }
        let start = align_up(region.start)?;
        let end = align_down(region.end);
        if start >= end {
            continue;
        }
        if count == MAX_USABLE_RANGES {
            return Err(PhysicalAllocatorError::TooManyUsableRanges);
        }
        if count > 0 {
            let previous = ranges[count - 1].expect("range count tracks initialized entries");
            if start < previous.end {
                return Err(PhysicalAllocatorError::OverlappingUsableRanges);
            }
        }
        ranges[count] = Some(PhysicalRange { start, end });
        count += 1;
    }
    Ok(ranges)
}

fn align_up(address: u64) -> Result<u64, PhysicalAllocatorError> {
    address
        .checked_add(PAGE_SIZE - 1)
        .map(|value| value & !(PAGE_SIZE - 1))
        .ok_or(PhysicalAllocatorError::AddressOverflow)
}

const fn align_down(address: u64) -> u64 {
    address & !(PAGE_SIZE - 1)
}

fn frame_from_address(address: u64) -> Result<PhysFrame, PhysicalAllocatorError> {
    let address =
        PhysAddr::try_new(address).map_err(|_| PhysicalAllocatorError::InvalidPhysicalAddress)?;
    PhysFrame::from_start_address(address)
        .map_err(|_| PhysicalAllocatorError::InvalidPhysicalAddress)
}

#[cfg(test)]
mod tests {
    use super::{BootReservations, PhysicalRange, SegmentedBitmapFrameAllocator};
    use crate::memory::boot::{BootContext, MemoryKind, MemoryRegion};

    fn context_with_usable_ranges(ranges: &[(u64, u64)]) -> BootContext {
        let mut regions = [MemoryRegion {
            start: 0,
            end: 0,
            source_type: 0,
            kind: MemoryKind::Reserved,
        }; 4];
        for (index, &(start, end)) in ranges.iter().enumerate() {
            regions[index] = MemoryRegion {
                start,
                end,
                source_type: 0,
                kind: MemoryKind::Usable,
            };
        }
        BootContext::for_test(&regions[..ranges.len()])
    }

    #[test]
    fn bitmap_skips_reserved_bootstrap_frames() {
        let context = context_with_usable_ranges(&[(0x1000, 0x5000)]);
        let mut reservations = BootReservations::new();
        reservations
            .reserve_range(PhysicalRange {
                start: 0x2000,
                end: 0x3000,
            })
            .unwrap();
        let mut bitmap = [0; 1];
        let mut allocator =
            SegmentedBitmapFrameAllocator::new(&context, &reservations, &mut bitmap).unwrap();

        assert_eq!(
            allocator.allocate().unwrap().start_address().as_u64(),
            0x1000
        );
        assert_eq!(
            allocator.allocate().unwrap().start_address().as_u64(),
            0x3000
        );
        assert_eq!(
            allocator.allocate().unwrap().start_address().as_u64(),
            0x4000
        );
        assert!(allocator.allocate().is_none());
    }

    #[test]
    fn bitmap_deallocation_makes_frame_available_again() {
        let context = context_with_usable_ranges(&[(0x1000, 0x3000)]);
        let mut bitmap = [0; 1];
        let mut allocator =
            SegmentedBitmapFrameAllocator::new(&context, &BootReservations::new(), &mut bitmap)
                .unwrap();
        let frame = allocator.allocate().unwrap();

        unsafe { allocator.deallocate(frame).unwrap() };

        assert_eq!(allocator.allocate().unwrap(), frame);
    }
}
