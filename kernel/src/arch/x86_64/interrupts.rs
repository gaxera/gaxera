//! BSP interrupt-vector ownership for the v1.2 legacy IOAPIC path.
//!
//! The table is deliberately a fixed-size atomic structure.  Allocation and
//! release happen in normal kernel context; the interrupt entry point only
//! performs bounded atomic loads/stores and never allocates or waits for a
//! lock.  A vector generation is returned to syscall-side owners so stale
//! control operations cannot affect a later allocation.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use kernel_core::object::ObjectId;

pub const DEVICE_VECTOR_FIRST: u8 = 0x40;
pub const DEVICE_VECTOR_LAST: u8 = 0xdf;
const VECTOR_COUNT: usize = (DEVICE_VECTOR_LAST - DEVICE_VECTOR_FIRST + 1) as usize;

const fn empty_generation() -> u32 {
    1
}

struct VectorSlot {
    active: AtomicBool,
    irq: AtomicU8,
    generation: AtomicU32,
    interrupt: AtomicU64,
    notification: AtomicU64,
    pending: AtomicU32,
}

impl VectorSlot {
    const fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            irq: AtomicU8::new(0),
            generation: AtomicU32::new(empty_generation()),
            interrupt: AtomicU64::new(0),
            notification: AtomicU64::new(0),
            pending: AtomicU32::new(0),
        }
    }
}

static VECTORS: [VectorSlot; VECTOR_COUNT] = [const { VectorSlot::new() }; VECTOR_COUNT];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VectorLease {
    vector: u8,
    generation: u32,
}

impl VectorLease {
    pub const fn from_parts(vector: u8, generation: u32) -> Self {
        Self { vector, generation }
    }

    pub const fn vector(self) -> u8 {
        self.vector
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VectorError {
    Exhausted,
    InvalidVector,
    StaleGeneration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DispatchRecord {
    pub lease: VectorLease,
    pub irq: u8,
    pub interrupt: ObjectId,
    pub notification: ObjectId,
}

const fn index(vector: u8) -> Option<usize> {
    if vector < DEVICE_VECTOR_FIRST || vector > DEVICE_VECTOR_LAST {
        None
    } else {
        Some((vector - DEVICE_VECTOR_FIRST) as usize)
    }
}

pub fn allocate(irq: u8, interrupt: ObjectId) -> Result<VectorLease, VectorError> {
    if irq >= 16 {
        return Err(VectorError::InvalidVector);
    }
    let slot = &VECTORS[irq as usize];
    if slot
        .active
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let generation = slot.generation.load(Ordering::Acquire);
        slot.irq.store(irq, Ordering::Release);
        slot.interrupt.store(interrupt.raw(), Ordering::Release);
        slot.notification.store(0, Ordering::Release);
        slot.pending.store(0, Ordering::Release);
        return Ok(VectorLease {
            vector: DEVICE_VECTOR_FIRST + irq,
            generation,
        });
    }
    /*
     * A legacy INTx line has one fixed vector in this first implementation.
     * Refusing a second object for the same line prevents two independent
     * capabilities from racing the same level-triggered controller state.
     */
    Err(VectorError::Exhausted)
}

pub fn release(lease: VectorLease) -> Result<(), VectorError> {
    let slot = VECTORS
        .get(index(lease.vector).ok_or(VectorError::InvalidVector)?)
        .ok_or(VectorError::InvalidVector)?;
    if slot.generation.load(Ordering::Acquire) != lease.generation
        || !slot.active.load(Ordering::Acquire)
    {
        return Err(VectorError::StaleGeneration);
    }

    // The caller masks the physical line before this publication.  Clearing
    // active first makes any late vector a no-op before metadata is recycled.
    slot.active.store(false, Ordering::Release);
    slot.interrupt.store(0, Ordering::Release);
    slot.notification.store(0, Ordering::Release);
    slot.pending.store(0, Ordering::Release);
    let next = slot
        .generation
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1);
    if next == 0 {
        slot.generation.store(1, Ordering::Release);
    }
    Ok(())
}

pub fn bind(lease: VectorLease, notification: ObjectId) -> Result<(), VectorError> {
    let slot = VECTORS
        .get(index(lease.vector).ok_or(VectorError::InvalidVector)?)
        .ok_or(VectorError::InvalidVector)?;
    if !slot.active.load(Ordering::Acquire)
        || slot.generation.load(Ordering::Acquire) != lease.generation
        || notification.raw() == 0
    {
        return Err(VectorError::StaleGeneration);
    }
    slot.notification
        .store(notification.raw(), Ordering::Release);
    Ok(())
}

pub fn unbind(lease: VectorLease) -> Result<(), VectorError> {
    let slot = VECTORS
        .get(index(lease.vector).ok_or(VectorError::InvalidVector)?)
        .ok_or(VectorError::InvalidVector)?;
    if !slot.active.load(Ordering::Acquire)
        || slot.generation.load(Ordering::Acquire) != lease.generation
    {
        return Err(VectorError::StaleGeneration);
    }
    slot.notification.store(0, Ordering::Release);
    slot.pending.store(0, Ordering::Release);
    Ok(())
}

pub fn lease_for(vector: u8) -> Option<VectorLease> {
    let slot = VECTORS.get(index(vector)?)?;
    if !slot.active.load(Ordering::Acquire) {
        return None;
    }
    Some(VectorLease {
        vector,
        generation: slot.generation.load(Ordering::Acquire),
    })
}

/// Resolve a hardware vector and mark one coalesced notification pending.
/// This is safe for the minimal interrupt handler: it performs no allocation,
/// no lock acquisition, and no unbounded traversal.
pub fn dispatch(vector: u8) -> Option<DispatchRecord> {
    let slot = VECTORS.get(index(vector)?)?;
    if !slot.active.load(Ordering::Acquire) {
        return None;
    }
    let notification = ObjectId::from_raw(slot.notification.load(Ordering::Acquire));
    if notification.raw() == 0 {
        return None;
    }
    slot.pending.fetch_or(1, Ordering::AcqRel);
    Some(DispatchRecord {
        lease: VectorLease {
            vector,
            generation: slot.generation.load(Ordering::Acquire),
        },
        irq: slot.irq.load(Ordering::Acquire),
        interrupt: ObjectId::from_raw(slot.interrupt.load(Ordering::Acquire)),
        notification,
    })
}

pub fn irq_for(vector: u8) -> Option<u8> {
    let slot = VECTORS.get(index(vector)?)?;
    if slot.active.load(Ordering::Acquire) {
        Some(slot.irq.load(Ordering::Acquire))
    } else {
        None
    }
}

pub fn mark_pending(lease: VectorLease) {
    if let Some(slot) = VECTORS.get(index(lease.vector).unwrap_or(usize::MAX))
        && slot.active.load(Ordering::Acquire)
        && slot.generation.load(Ordering::Acquire) == lease.generation
    {
        slot.pending.fetch_or(1, Ordering::AcqRel);
    }
}

pub fn take_pending(lease: VectorLease) -> Option<ObjectId> {
    let slot = VECTORS.get(index(lease.vector)?)?;
    if !slot.active.load(Ordering::Acquire)
        || slot.generation.load(Ordering::Acquire) != lease.generation
    {
        return None;
    }
    if slot.pending.swap(0, Ordering::AcqRel) == 0 {
        return None;
    }
    Some(ObjectId::from_raw(
        slot.notification.load(Ordering::Acquire),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocation_dispatch_and_generation_reuse() {
        let notification = ObjectId::new_for_test(10, 1);
        let interrupt = ObjectId::new_for_test(11, 1);
        let first = allocate(11, interrupt).unwrap();
        bind(first, notification).unwrap();
        assert_eq!(dispatch(first.vector()).unwrap().notification, notification);
        assert_eq!(take_pending(first), Some(notification));
        release(first).unwrap();

        let second = allocate(11, interrupt).unwrap();
        bind(second, notification).unwrap();
        assert_ne!(first.generation(), second.generation());
        assert_eq!(take_pending(first), None);
        release(second).unwrap();
    }
}
