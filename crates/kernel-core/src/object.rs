use alloc::vec::Vec;

use gaxera_abi::{ObjectType, ObjectTypeSet};

use crate::resource::{ResourceDomain, ResourceDomainId, ResourceError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectId {
    index: u32,
    generation: u32,
}

impl ObjectId {
    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }

    #[doc(hidden)]
    pub const fn new_for_test(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Factory {
    domain: ResourceDomainId,
    allowed_types: ObjectTypeSet,
}

impl Factory {
    // Factory-capability lookup is introduced with the kernel object store.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn new(domain: &ResourceDomain, allowed_types: ObjectTypeSet) -> Self {
        Self {
            domain: domain.id(),
            allowed_types,
        }
    }

    pub const fn domain(&self) -> ResourceDomainId {
        self.domain
    }

    pub const fn allows(&self, object_type: ObjectType) -> bool {
        self.allowed_types.contains(object_type)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArenaInitError {
    CapacityTooLarge,
    AllocationFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectError {
    ArenaFull,
    FactoryDomainMismatch,
    FactoryDenied,
    ObjectNotFound,
    OwnerDomainMismatch,
    Resource(ResourceError),
}

impl From<ResourceError> for ObjectError {
    fn from(error: ResourceError) -> Self {
        Self::Resource(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ObjectSlotState {
    Free {
        next: Option<u32>,
    },
    Live {
        object_type: ObjectType,
        owner: ResourceDomainId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObjectSlot {
    generation: u32,
    state: ObjectSlotState,
}

/// Fallible typed object storage.
///
/// Identity is the stable index/generation pair, never a storage address. The
/// vector is reserved once during setup, so M1 allocation paths do not grow it.
pub struct ObjectArena {
    slots: Vec<ObjectSlot>,
    capacity: usize,
    free_head: Option<u32>,
}

impl ObjectArena {
    pub fn try_new(capacity: usize) -> Result<Self, ArenaInitError> {
        if capacity > u32::MAX as usize {
            return Err(ArenaInitError::CapacityTooLarge);
        }
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(capacity)
            .map_err(|_| ArenaInitError::AllocationFailed)?;
        Ok(Self {
            slots,
            capacity,
            free_head: None,
        })
    }

    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    pub const fn allocated_slots(&self) -> usize {
        self.slots.len()
    }

    pub fn create(
        &mut self,
        domain: &mut ResourceDomain,
        factory: Factory,
        object_type: ObjectType,
    ) -> Result<ObjectId, ObjectError> {
        if factory.domain() != domain.id() {
            return Err(ObjectError::FactoryDomainMismatch);
        }
        if !factory.allows(object_type) {
            return Err(ObjectError::FactoryDenied);
        }
        if self.free_head.is_none() && self.slots.len() == self.capacity {
            return Err(ObjectError::ArenaFull);
        }

        domain.charge_object()?;
        if let Some(index) = self.free_head {
            let slot = match self.slots.get_mut(index as usize) {
                Some(slot) => slot,
                None => {
                    let _ = domain.release_object();
                    return Err(ObjectError::ObjectNotFound);
                }
            };
            let next = match slot.state {
                ObjectSlotState::Free { next } => next,
                ObjectSlotState::Live { .. } => {
                    let _ = domain.release_object();
                    return Err(ObjectError::ObjectNotFound);
                }
            };
            self.free_head = next;
            slot.state = ObjectSlotState::Live {
                object_type,
                owner: domain.id(),
            };
            return Ok(ObjectId {
                index,
                generation: slot.generation,
            });
        }

        let index = self.slots.len() as u32;
        let generation = 1;
        self.slots.push(ObjectSlot {
            generation,
            state: ObjectSlotState::Live {
                object_type,
                owner: domain.id(),
            },
        });
        Ok(ObjectId { index, generation })
    }

    pub fn destroy(
        &mut self,
        domain: &mut ResourceDomain,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        let slot = self
            .slots
            .get_mut(object.index as usize)
            .ok_or(ObjectError::ObjectNotFound)?;
        let owner = match slot.state {
            ObjectSlotState::Live { owner, .. } if slot.generation == object.generation => owner,
            _ => return Err(ObjectError::ObjectNotFound),
        };
        if owner != domain.id() {
            return Err(ObjectError::OwnerDomainMismatch);
        }
        domain.release_object()?;
        slot.generation = next_generation(slot.generation);
        slot.state = ObjectSlotState::Free {
            next: self.free_head,
        };
        self.free_head = Some(object.index);
        Ok(())
    }

    pub fn object_type(&self, object: ObjectId) -> Option<ObjectType> {
        let slot = self.slots.get(object.index as usize)?;
        match slot.state {
            ObjectSlotState::Live { object_type, .. } if slot.generation == object.generation => {
                Some(object_type)
            }
            _ => None,
        }
    }

    pub fn is_live(&self, object: ObjectId) -> bool {
        self.object_type(object).is_some()
    }
}

const fn next_generation(generation: u32) -> u32 {
    let next = generation.wrapping_add(1);
    if next == 0 { 1 } else { next }
}
