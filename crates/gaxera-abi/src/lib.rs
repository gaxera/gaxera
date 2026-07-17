#![no_std]

use core::ops::{BitAnd, BitOr, BitOrAssign};

/// Opaque capability handle carried across the future user/kernel ABI.
///
/// A caller may manufacture raw bits, but the kernel accepts a handle only
/// after capability-space generation, object, rights, and lineage validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Handle(u64);

impl Handle {
    pub const INVALID: Self = Self(0);

    pub const fn from_parts(slot: u32, generation: u32) -> Self {
        Self(((generation as u64) << 32) | (slot as u64))
    }

    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    pub const fn slot(self) -> u32 {
        self.0 as u32
    }

    pub const fn generation(self) -> u32 {
        (self.0 >> 32) as u32
    }

    pub const fn is_valid(self) -> bool {
        self.0 != 0
    }
}

/// The complete long-term kernel object taxonomy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ObjectType {
    Thread = 0,
    AddressSpace = 1,
    CapabilitySpace = 2,
    Endpoint = 3,
    Notification = 4,
    MemoryObject = 5,
    Mapping = 6,
    InterruptObject = 7,
    TimerObject = 8,
    SchedulingContext = 9,
    ResourceDomain = 10,
}

impl ObjectType {
    pub const fn bit(self) -> u16 {
        1_u16 << (self as u8)
    }
}

/// A set of object kinds authorized by a Factory right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ObjectTypeSet(u16);

impl ObjectTypeSet {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self((1_u16 << 11) - 1);

    pub const fn of(object_type: ObjectType) -> Self {
        Self(object_type.bit())
    }

    pub const fn contains(self, object_type: ObjectType) -> bool {
        (self.0 & object_type.bit()) != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Object-specific authority bits. A rights value may only be narrowed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Rights(u32);

impl Rights {
    pub const NONE: Self = Self(0);
    pub const READ: Self = Self(1 << 0);
    pub const WRITE: Self = Self(1 << 1);
    pub const MAP: Self = Self(1 << 2);
    pub const EXECUTE: Self = Self(1 << 3);
    pub const SIGNAL: Self = Self(1 << 4);
    pub const WAIT: Self = Self(1 << 5);
    pub const MANAGE: Self = Self(1 << 6);
    pub const FACTORY: Self = Self(1 << 7);

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, required: Self) -> bool {
        (self.0 & required.0) == required.0
    }

    pub const fn is_subset_of(self, other: Self) -> bool {
        (self.0 & !other.0) == 0
    }
}

impl BitAnd for Rights {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitOr for Rights {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Rights {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
