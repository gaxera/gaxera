use alloc::vec::Vec;

use gaxera_abi::{Handle, ObjectType, Rights};

use crate::object::{ObjectArena, ObjectId};
use crate::resource::{ResourceDomain, ResourceDomainId, ResourceError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityInitError {
    CapacityTooLarge,
    AllocationFailed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityError {
    DomainMismatch,
    SpaceFull,
    NodeCapacity,
    InvalidHandle,
    StaleHandle,
    Revoked,
    ObjectDestroyed,
    TypeMismatch,
    RightsDenied,
    RightsEscalation,
    Resource(ResourceError),
    AccountingInvariant,
}

impl From<ResourceError> for CapabilityError {
    fn from(error: ResourceError) -> Self {
        Self::Resource(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NodeId {
    index: u32,
    generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CapabilityInfo {
    object: ObjectId,
    object_type: ObjectType,
    rights: Rights,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NodeState {
    Free {
        next: Option<u32>,
    },
    Live {
        info: CapabilityInfo,
        parent: Option<NodeId>,
        revoked: bool,
        references: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CapabilityNode {
    generation: u32,
    state: NodeState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CapabilitySlot {
    generation: u32,
    node: Option<NodeId>,
    next_free: Option<u32>,
}

/// A per-domain capability space. Slots are private; callers interact only
/// through opaque generational ABI handles.
pub struct CapabilitySpace {
    domain: ResourceDomainId,
    slots: Vec<CapabilitySlot>,
    capacity: usize,
    free_head: Option<u32>,
}

impl CapabilitySpace {
    pub fn try_new(domain: &ResourceDomain, capacity: usize) -> Result<Self, CapabilityInitError> {
        if capacity > u32::MAX as usize {
            return Err(CapabilityInitError::CapacityTooLarge);
        }
        let mut slots = Vec::new();
        slots
            .try_reserve_exact(capacity)
            .map_err(|_| CapabilityInitError::AllocationFailed)?;
        Ok(Self {
            domain: domain.id(),
            slots,
            capacity,
            free_head: None,
        })
    }

    pub const fn domain(&self) -> ResourceDomainId {
        self.domain
    }

    fn has_capacity(&self) -> bool {
        self.free_head.is_some() || self.slots.len() < self.capacity
    }

    fn insert(&mut self, node: NodeId) -> Result<Handle, CapabilityError> {
        if let Some(index) = self.free_head {
            let slot = self
                .slots
                .get_mut(index as usize)
                .ok_or(CapabilityError::AccountingInvariant)?;
            self.free_head = slot.next_free;
            slot.node = Some(node);
            slot.next_free = None;
            return Ok(Handle::from_parts(index, slot.generation));
        }
        if self.slots.len() == self.capacity {
            return Err(CapabilityError::SpaceFull);
        }
        let index = self.slots.len() as u32;
        let generation = 1;
        self.slots.push(CapabilitySlot {
            generation,
            node: Some(node),
            next_free: None,
        });
        Ok(Handle::from_parts(index, generation))
    }

    fn remove(&mut self, handle: Handle) -> Result<NodeId, CapabilityError> {
        if !handle.is_valid() {
            return Err(CapabilityError::InvalidHandle);
        }
        let slot = self
            .slots
            .get_mut(handle.slot() as usize)
            .ok_or(CapabilityError::InvalidHandle)?;
        if slot.generation != handle.generation() {
            return Err(CapabilityError::StaleHandle);
        }
        let node = slot.node.take().ok_or(CapabilityError::StaleHandle)?;
        slot.generation = next_generation(slot.generation);
        slot.next_free = self.free_head;
        self.free_head = Some(handle.slot());
        Ok(node)
    }

    fn node_for(&self, handle: Handle) -> Result<NodeId, CapabilityError> {
        if !handle.is_valid() {
            return Err(CapabilityError::InvalidHandle);
        }
        let slot = self
            .slots
            .get(handle.slot() as usize)
            .ok_or(CapabilityError::InvalidHandle)?;
        if slot.generation != handle.generation() {
            return Err(CapabilityError::StaleHandle);
        }
        slot.node.ok_or(CapabilityError::StaleHandle)
    }
}

/// Capability nodes are global to this model so descendants may move across
/// capability spaces without losing derivation lineage.
pub struct CapabilitySystem {
    nodes: Vec<CapabilityNode>,
    capacity: usize,
    free_head: Option<u32>,
}

impl CapabilitySystem {
    pub fn try_new(capacity: usize) -> Result<Self, CapabilityInitError> {
        if capacity > u32::MAX as usize {
            return Err(CapabilityInitError::CapacityTooLarge);
        }
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(capacity)
            .map_err(|_| CapabilityInitError::AllocationFailed)?;
        Ok(Self {
            nodes,
            capacity,
            free_head: None,
        })
    }

    pub fn insert_root(
        &mut self,
        target: &mut CapabilitySpace,
        domain: &mut ResourceDomain,
        object: ObjectId,
        object_type: ObjectType,
        rights: Rights,
        objects: &ObjectArena,
    ) -> Result<Handle, CapabilityError> {
        if target.domain() != domain.id() {
            return Err(CapabilityError::DomainMismatch);
        }
        if objects.object_type(object) != Some(object_type) {
            return Err(CapabilityError::ObjectDestroyed);
        }
        self.insert_new(
            target,
            domain,
            CapabilityInfo {
                object,
                object_type,
                rights,
            },
            None,
        )
    }

    pub fn lookup(
        &self,
        space: &CapabilitySpace,
        handle: Handle,
        required_type: ObjectType,
        required_rights: Rights,
        objects: &ObjectArena,
    ) -> Result<ObjectId, CapabilityError> {
        let node = space.node_for(handle)?;
        let info = self.validate_node(node, objects)?;
        if info.object_type != required_type {
            return Err(CapabilityError::TypeMismatch);
        }
        if !info.rights.contains(required_rights) {
            return Err(CapabilityError::RightsDenied);
        }
        Ok(info.object)
    }

    pub fn derive(
        &mut self,
        source: &CapabilitySpace,
        source_handle: Handle,
        target: &mut CapabilitySpace,
        target_domain: &mut ResourceDomain,
        requested_rights: Rights,
        objects: &ObjectArena,
    ) -> Result<Handle, CapabilityError> {
        let parent = source.node_for(source_handle)?;
        let info = self.validate_node(parent, objects)?;
        if !requested_rights.is_subset_of(info.rights) {
            return Err(CapabilityError::RightsEscalation);
        }
        self.insert_new(
            target,
            target_domain,
            CapabilityInfo {
                rights: requested_rights,
                ..info
            },
            Some(parent),
        )
    }

    pub fn revoke(
        &mut self,
        space: &CapabilitySpace,
        handle: Handle,
        objects: &ObjectArena,
    ) -> Result<(), CapabilityError> {
        let node = space.node_for(handle)?;
        let info = self.validate_node(node, objects)?;
        if !info.rights.contains(Rights::MANAGE) {
            return Err(CapabilityError::RightsDenied);
        }
        let node = self.node_mut(node)?;
        match &mut node.state {
            NodeState::Live { revoked, .. } => {
                *revoked = true;
                Ok(())
            }
            NodeState::Free { .. } => Err(CapabilityError::StaleHandle),
        }
    }

    pub fn delete(
        &mut self,
        space: &mut CapabilitySpace,
        domain: &mut ResourceDomain,
        handle: Handle,
    ) -> Result<(), CapabilityError> {
        if space.domain() != domain.id() {
            return Err(CapabilityError::DomainMismatch);
        }
        let node = space.remove(handle)?;
        domain.release_capability()?;
        self.release_node(node)
    }

    pub fn prepare_transfer(
        &self,
        source: &CapabilitySpace,
        handle: Handle,
        requested_rights: Rights,
        objects: &ObjectArena,
    ) -> Result<PreparedTransfer, CapabilityError> {
        let node = source.node_for(handle)?;
        let info = self.validate_node(node, objects)?;
        if !requested_rights.is_subset_of(info.rights) {
            return Err(CapabilityError::RightsEscalation);
        }
        Ok(PreparedTransfer {
            source_handle: handle,
            requested_rights,
        })
    }

    pub fn commit_transfer(
        &mut self,
        source: &CapabilitySpace,
        prepared: PreparedTransfer,
        target: &mut CapabilitySpace,
        target_domain: &mut ResourceDomain,
        objects: &ObjectArena,
    ) -> Result<Handle, CapabilityError> {
        self.derive(
            source,
            prepared.source_handle,
            target,
            target_domain,
            prepared.requested_rights,
            objects,
        )
    }

    fn insert_new(
        &mut self,
        target: &mut CapabilitySpace,
        domain: &mut ResourceDomain,
        info: CapabilityInfo,
        parent: Option<NodeId>,
    ) -> Result<Handle, CapabilityError> {
        if target.domain() != domain.id() {
            return Err(CapabilityError::DomainMismatch);
        }
        if !target.has_capacity() {
            return Err(CapabilityError::SpaceFull);
        }
        if self.free_head.is_none() && self.nodes.len() == self.capacity {
            return Err(CapabilityError::NodeCapacity);
        }
        domain.charge_capability()?;
        let node = match self.allocate_node(info, parent) {
            Ok(node) => node,
            Err(error) => {
                let _ = domain.release_capability();
                return Err(error);
            }
        };
        match target.insert(node) {
            Ok(handle) => Ok(handle),
            Err(error) => {
                let _ = self.release_node(node);
                let _ = domain.release_capability();
                Err(error)
            }
        }
    }

    fn allocate_node(
        &mut self,
        info: CapabilityInfo,
        parent: Option<NodeId>,
    ) -> Result<NodeId, CapabilityError> {
        if let Some(parent) = parent {
            self.add_reference(parent)?;
        }
        if let Some(index) = self.free_head {
            let node = self
                .nodes
                .get_mut(index as usize)
                .ok_or(CapabilityError::AccountingInvariant)?;
            let next = match node.state {
                NodeState::Free { next } => next,
                NodeState::Live { .. } => return Err(CapabilityError::AccountingInvariant),
            };
            self.free_head = next;
            node.state = NodeState::Live {
                info,
                parent,
                revoked: false,
                references: 1,
            };
            return Ok(NodeId {
                index,
                generation: node.generation,
            });
        }
        if self.nodes.len() == self.capacity {
            return Err(CapabilityError::NodeCapacity);
        }
        let index = self.nodes.len() as u32;
        let generation = 1;
        self.nodes.push(CapabilityNode {
            generation,
            state: NodeState::Live {
                info,
                parent,
                revoked: false,
                references: 1,
            },
        });
        Ok(NodeId { index, generation })
    }

    fn validate_node(
        &self,
        mut node: NodeId,
        objects: &ObjectArena,
    ) -> Result<CapabilityInfo, CapabilityError> {
        let mut depth = 0_usize;
        let mut leaf = None;
        loop {
            let current = self.node(node)?;
            let (info, parent, revoked) = match current.state {
                NodeState::Live {
                    info,
                    parent,
                    revoked,
                    ..
                } => (info, parent, revoked),
                NodeState::Free { .. } => return Err(CapabilityError::StaleHandle),
            };
            if revoked {
                return Err(CapabilityError::Revoked);
            }
            if leaf.is_none() {
                leaf = Some(info);
            }
            depth += 1;
            if depth > self.capacity {
                return Err(CapabilityError::AccountingInvariant);
            }
            match parent {
                Some(parent) => node = parent,
                None => {
                    let leaf = leaf.ok_or(CapabilityError::AccountingInvariant)?;
                    if objects.object_type(leaf.object) != Some(leaf.object_type) {
                        return Err(CapabilityError::ObjectDestroyed);
                    }
                    return Ok(leaf);
                }
            }
        }
    }

    fn add_reference(&mut self, id: NodeId) -> Result<(), CapabilityError> {
        let node = self.node_mut(id)?;
        match &mut node.state {
            NodeState::Live { references, .. } => {
                *references = references
                    .checked_add(1)
                    .ok_or(CapabilityError::AccountingInvariant)?;
                Ok(())
            }
            NodeState::Free { .. } => Err(CapabilityError::StaleHandle),
        }
    }

    fn release_node(&mut self, mut id: NodeId) -> Result<(), CapabilityError> {
        loop {
            let parent = {
                let node = self.node_mut(id)?;
                match &mut node.state {
                    NodeState::Live {
                        parent, references, ..
                    } => {
                        if *references == 0 {
                            return Err(CapabilityError::AccountingInvariant);
                        }
                        *references -= 1;
                        if *references != 0 {
                            return Ok(());
                        }
                        *parent
                    }
                    NodeState::Free { .. } => return Err(CapabilityError::StaleHandle),
                }
            };
            let free_head = self.free_head;
            let node = self.node_mut(id)?;
            node.generation = next_generation(node.generation);
            node.state = NodeState::Free { next: free_head };
            self.free_head = Some(id.index);
            match parent {
                Some(parent) => id = parent,
                None => return Ok(()),
            }
        }
    }

    fn node(&self, id: NodeId) -> Result<&CapabilityNode, CapabilityError> {
        let node = self
            .nodes
            .get(id.index as usize)
            .ok_or(CapabilityError::StaleHandle)?;
        if node.generation != id.generation {
            return Err(CapabilityError::StaleHandle);
        }
        Ok(node)
    }

    fn node_mut(&mut self, id: NodeId) -> Result<&mut CapabilityNode, CapabilityError> {
        let node = self
            .nodes
            .get_mut(id.index as usize)
            .ok_or(CapabilityError::StaleHandle)?;
        if node.generation != id.generation {
            return Err(CapabilityError::StaleHandle);
        }
        Ok(node)
    }
}

/// A transfer is intentionally allocation-free. Commit validates its source
/// again, so revocation or deletion between prepare and commit cannot transfer
/// stale authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedTransfer {
    source_handle: Handle,
    requested_rights: Rights,
}

const fn next_generation(generation: u32) -> u32 {
    let next = generation.wrapping_add(1);
    if next == 0 { 1 } else { next }
}
