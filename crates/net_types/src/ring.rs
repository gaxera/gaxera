//! Shared Memory PacketRing and Backpressure Mechanics.

use core::sync::atomic::{AtomicU32, Ordering};

/// Backpressure Policies for PacketRing.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum BackpressurePolicy {
    FlowControl = 0,     // RX Ring: Signal TCP window shrink
    DropOldest = 1,      // RX Ring: Evict oldest unconsumed frame
    BlockProducer = 2,   // TX Ring: Block application thread
    DropNewest = 3,      // TX Ring: Reject incoming write frame
    PriorityDiscard = 4, // Control Ring: Drop non-essential telemetry
}

/// PacketRing Type Role.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[repr(u8)]
pub enum RingType {
    Rx = 0,
    Tx = 1,
    Control = 2,
}

impl RingType {
    /// Return the canonical backpressure policy for this ring type.
    pub fn default_policy(&self) -> BackpressurePolicy {
        match self {
            Self::Rx => BackpressurePolicy::FlowControl,
            Self::Tx => BackpressurePolicy::BlockProducer,
            Self::Control => BackpressurePolicy::PriorityDiscard,
        }
    }
}

/// Shared Memory PacketRing Header.
#[derive(Debug)]
#[repr(C)]
pub struct PacketRingHeader {
    pub magic: u32,
    pub version: u32,
    pub capacity: u32, // MUST be a power of two
    pub ring_type: u8,
    pub policy: u8,
    pub reserved: u16,
    pub producer_index: AtomicU32,
    pub consumer_index: AtomicU32,
}

impl PacketRingHeader {
    pub const MAGIC: u32 = 0x504B5452; // b"PKTR"

    pub fn new(capacity: u32, ring_type: RingType) -> Self {
        assert!(capacity.is_power_of_two(), "Capacity must be power of two");
        Self {
            magic: Self::MAGIC,
            version: 1,
            capacity,
            ring_type: ring_type as u8,
            policy: ring_type.default_policy() as u8,
            reserved: 0,
            producer_index: AtomicU32::new(0),
            consumer_index: AtomicU32::new(0),
        }
    }

    /// Calculate current number of queued unconsumed frames.
    pub fn len(&self) -> u32 {
        let prod = self.producer_index.load(Ordering::Acquire);
        let cons = self.consumer_index.load(Ordering::Acquire);
        prod.wrapping_sub(cons)
    }

    /// Check if the ring buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if the ring buffer is full.
    pub fn is_full(&self) -> bool {
        self.len() >= self.capacity
    }

    /// Submit a new slot: returns physical index slot if space is available.
    pub fn push_slot(&self) -> Result<u32, BackpressurePolicy> {
        let prod = self.producer_index.load(Ordering::Relaxed);
        let cons = self.consumer_index.load(Ordering::Acquire);
        if prod.wrapping_sub(cons) >= self.capacity {
            let policy = match self.policy {
                0 => BackpressurePolicy::FlowControl,
                1 => BackpressurePolicy::DropOldest,
                2 => BackpressurePolicy::BlockProducer,
                3 => BackpressurePolicy::DropNewest,
                _ => BackpressurePolicy::PriorityDiscard,
            };
            return Err(policy);
        }
        let slot = prod & (self.capacity - 1);
        self.producer_index
            .store(prod.wrapping_add(1), Ordering::Release);
        Ok(slot)
    }

    /// Consume a slot: returns physical index slot if data is available.
    pub fn pop_slot(&self) -> Option<u32> {
        let cons = self.consumer_index.load(Ordering::Relaxed);
        let prod = self.producer_index.load(Ordering::Acquire);
        if cons == prod {
            return None;
        }
        let slot = cons & (self.capacity - 1);
        self.consumer_index
            .store(cons.wrapping_add(1), Ordering::Release);
        Some(slot)
    }
}
