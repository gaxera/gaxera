#![no_std]

pub mod block;
pub mod boot;
pub mod ipc;
pub mod net;
pub mod pci;
pub mod service;

pub mod svc {
    pub const RAMFS_BASE: u64 = 0x0000_6000_0000_0000;
    pub const RAMFS_MAX_SIZE: usize = 16 * 1024 * 1024;
    pub const ENDPOINT_RAMFS: u64 = 0x100000000;
    pub const ENDPOINT_CONSOLE: u64 = 0x100000001;
    pub const STATUS_OK: u64 = 0;
    pub const STATUS_NOT_FOUND: u64 = 1;
}

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
    DebugConsole = 11,
    Factory = 12,
    WaitSet = 13,
    ContiguousFrame = 14,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u64)]
pub enum OperationCode {
    YieldProcess = 0,
    MapMemory = 1,
    Call = 2,
    Receive = 3,
    Reply = 4,
    ConfigureThread = 5,
    Write = 6,
    Derive = 7,
    ThreadStatus = 8,
    UnmapMemory = 9,
    CreateWaitSet = 10,
    WaitSetControl = 11,
    WaitSetWait = 12,
    DeleteHandle = 13,
    Revoke = 14,
    InterruptControl = 15,
    WaitNotification = 16,
    ExitProcess = 99,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum InterruptOp {
    BindNotification = 1,
    Mask = 2,
    Unmask = 3,
    Ack = 4,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CachePolicy {
    Cached = 0,
    Uncached = 1,
    WriteThrough = 2,
    WriteCombining = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum WaitSetOp {
    Add = 1,
    Remove = 2,
    Modify = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct WaitSetEvent {
    pub cookie: u64,
    pub signals: u32,
    pub _reserved: u32,
}

pub const THREAD_STATE_RUNNABLE_OR_RUNNING: u64 = 0;
pub const THREAD_STATE_DEAD: u64 = 1;

impl ObjectType {
    pub const fn bit(self) -> u16 {
        1_u16 << (self as u8)
    }
}

impl core::convert::TryFrom<u32> for ObjectType {
    type Error = ();
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Thread),
            1 => Ok(Self::AddressSpace),
            2 => Ok(Self::CapabilitySpace),
            3 => Ok(Self::Endpoint),
            4 => Ok(Self::Notification),
            5 => Ok(Self::MemoryObject),
            6 => Ok(Self::Mapping),
            7 => Ok(Self::InterruptObject),
            8 => Ok(Self::TimerObject),
            9 => Ok(Self::SchedulingContext),
            10 => Ok(Self::ResourceDomain),
            11 => Ok(Self::DebugConsole),
            12 => Ok(Self::Factory),
            13 => Ok(Self::WaitSet),
            14 => Ok(Self::ContiguousFrame),
            _ => Err(()),
        }
    }
}

/// A set of object kinds authorized by a Factory right.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ObjectTypeSet(u16);

impl ObjectTypeSet {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self((1_u16 << (ObjectType::ContiguousFrame as u8 + 1)) - 1);

    pub const fn of(object_type: ObjectType) -> Self {
        Self(1 << (object_type as u8))
    }

    pub const fn from_bits(bits: u16) -> Self {
        Self(bits)
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
    pub const ALL: Self = Self(0xFFFF_FFFF);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_object_type_set_completeness() {
        for discriminant in 0..=14 {
            let obj = ObjectType::try_from(discriminant).expect("Valid ObjectType discriminant");
            assert!(
                ObjectTypeSet::ALL.contains(obj),
                "ObjectTypeSet::ALL failed to contain {:?}",
                obj
            );
        }
    }
}
