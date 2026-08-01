#![no_std]

extern crate alloc;

pub mod address_space;
pub mod affinity;

pub mod capability;
pub mod contiguous_frame;
pub mod debug_console;
pub mod elf;
pub mod interrupt;
pub mod ipc;
pub mod mapping;
pub mod memory;
pub mod notification;
pub mod object;
pub mod registry;
pub mod resource;
pub mod scheduler;
pub mod scheduler_domain;
pub mod slab;
pub mod thread;
pub mod time;
pub mod timer;
pub mod waitset;

#[cfg(test)]
mod tests {
    use gaxera_abi::{ObjectType, ObjectTypeSet, Rights};

    use crate::capability::{CapabilityError, CapabilitySpace, CapabilitySystem};
    use crate::object::{Factory, ObjectArena, ObjectError};
    use crate::resource::{ResourceDomain, ResourceDomainId, ResourceError, ResourceLimits};

    const DOMAIN_A: ResourceDomainId = ResourceDomainId::new(1);
    const DOMAIN_B: ResourceDomainId = ResourceDomainId::new(2);

    fn domain(id: ResourceDomainId, objects: u32, capabilities: u32) -> ResourceDomain {
        ResourceDomain::new(
            id,
            ResourceLimits {
                objects,
                capabilities,
                memory_bytes: 65536,
            },
        )
    }

    fn endpoint_factory(domain: &ResourceDomain) -> Factory {
        Factory::new(domain, ObjectTypeSet::of(ObjectType::Endpoint))
    }

    fn endpoint(arena: &mut ObjectArena, domain: &mut ResourceDomain) -> crate::object::ObjectId {
        arena
            .create(domain, endpoint_factory(domain), ObjectType::Endpoint)
            .unwrap()
    }

    #[test]
    fn object_creation_is_fallible_and_reuse_invalidates_stale_identity() {
        let mut domain = domain(DOMAIN_A, 1, 4);
        let mut arena = ObjectArena::try_new(1).unwrap();
        let factory = endpoint_factory(&domain);

        assert_eq!(
            arena.create(&mut domain, factory, ObjectType::Thread),
            Err(ObjectError::FactoryDenied)
        );
        assert_eq!(domain.usage().objects, 0);

        let first = arena
            .create(&mut domain, factory, ObjectType::Endpoint)
            .unwrap();
        assert_eq!(domain.usage().objects, 1);
        assert_eq!(
            arena.create(&mut domain, factory, ObjectType::Endpoint),
            Err(ObjectError::ArenaFull)
        );
        assert_eq!(domain.usage().objects, 1);

        arena.destroy(&mut domain, first).unwrap();
        assert_eq!(domain.usage().objects, 0);
        let second = arena
            .create(&mut domain, factory, ObjectType::Endpoint)
            .unwrap();
        assert_eq!(first.index(), second.index());
        assert_ne!(first.generation(), second.generation());
        assert!(!arena.is_live(first));
        assert!(arena.is_live(second));
    }

    #[test]
    fn resource_limits_are_recoverable_errors() {
        let mut domain = domain(DOMAIN_A, 1, 1);
        assert_eq!(
            domain.release_object(),
            Err(ResourceError::AccountingUnderflow)
        );
        domain.charge_object().unwrap();
        assert_eq!(domain.charge_object(), Err(ResourceError::ObjectLimit));
        domain.release_object().unwrap();
        domain.charge_capability().unwrap();
        assert_eq!(
            domain.charge_capability(),
            Err(ResourceError::CapabilityLimit)
        );
        domain.release_capability().unwrap();
        assert_eq!(domain.usage().objects, 0);
        assert_eq!(domain.usage().capabilities, 0);
    }

    #[test]
    fn derivation_can_only_narrow_rights() {
        let mut owner = domain(DOMAIN_A, 2, 8);
        let mut recipient = domain(DOMAIN_B, 2, 8);
        let mut arena = ObjectArena::try_new(2).unwrap();
        let object = endpoint(&mut arena, &mut owner);
        let mut source = CapabilitySpace::try_new(&owner, 4).unwrap();
        let mut target = CapabilitySpace::try_new(&recipient, 4).unwrap();
        let mut system = CapabilitySystem::try_new(8).unwrap();
        let root_rights = Rights::READ | Rights::WRITE | Rights::MANAGE;
        let root = system
            .insert_root(
                &mut source,
                &mut owner,
                object,
                ObjectType::Endpoint,
                root_rights,
                &arena,
            )
            .unwrap();
        let child = system
            .derive(
                &source,
                root,
                &mut target,
                &mut recipient,
                Rights::READ,
                &arena,
            )
            .unwrap();

        assert_eq!(
            system.lookup(&target, child, ObjectType::Endpoint, Rights::WRITE, &arena,),
            Err(CapabilityError::RightsDenied)
        );
        assert_eq!(
            system.derive(
                &target,
                child,
                &mut source,
                &mut owner,
                Rights::READ | Rights::WRITE,
                &arena,
            ),
            Err(CapabilityError::RightsEscalation)
        );
    }

    #[test]
    fn revocation_invalidates_descendants_across_spaces() {
        let mut owner = domain(DOMAIN_A, 2, 8);
        let mut recipient = domain(DOMAIN_B, 2, 8);
        let mut arena = ObjectArena::try_new(2).unwrap();
        let object = endpoint(&mut arena, &mut owner);
        let mut source = CapabilitySpace::try_new(&owner, 4).unwrap();
        let mut target = CapabilitySpace::try_new(&recipient, 4).unwrap();
        let mut system = CapabilitySystem::try_new(8).unwrap();
        let root = system
            .insert_root(
                &mut source,
                &mut owner,
                object,
                ObjectType::Endpoint,
                Rights::READ | Rights::MANAGE,
                &arena,
            )
            .unwrap();
        let child = system
            .derive(
                &source,
                root,
                &mut target,
                &mut recipient,
                Rights::READ,
                &arena,
            )
            .unwrap();

        system.revoke(&source, root, &arena).unwrap();
        assert_eq!(
            system.lookup(&target, child, ObjectType::Endpoint, Rights::READ, &arena,),
            Err(CapabilityError::Revoked)
        );
    }

    #[test]
    fn deleting_a_parent_handle_does_not_revoke_a_child() {
        let mut owner = domain(DOMAIN_A, 2, 8);
        let mut recipient = domain(DOMAIN_B, 2, 8);
        let mut arena = ObjectArena::try_new(2).unwrap();
        let object = endpoint(&mut arena, &mut owner);
        let mut source = CapabilitySpace::try_new(&owner, 4).unwrap();
        let mut target = CapabilitySpace::try_new(&recipient, 4).unwrap();
        let mut system = CapabilitySystem::try_new(8).unwrap();
        let root = system
            .insert_root(
                &mut source,
                &mut owner,
                object,
                ObjectType::Endpoint,
                Rights::READ,
                &arena,
            )
            .unwrap();
        let child = system
            .derive(
                &source,
                root,
                &mut target,
                &mut recipient,
                Rights::READ,
                &arena,
            )
            .unwrap();

        system.delete(&mut source, &mut owner, root).unwrap();
        assert_eq!(owner.usage().capabilities, 0);
        assert_eq!(
            system.lookup(&target, child, ObjectType::Endpoint, Rights::READ, &arena,),
            Ok(object)
        );
    }

    #[test]
    fn prepared_transfer_rolls_back_when_target_cannot_accept_it() {
        let mut owner = domain(DOMAIN_A, 2, 8);
        let mut recipient = domain(DOMAIN_B, 2, 0);
        let mut arena = ObjectArena::try_new(2).unwrap();
        let object = endpoint(&mut arena, &mut owner);
        let mut source = CapabilitySpace::try_new(&owner, 4).unwrap();
        let mut target = CapabilitySpace::try_new(&recipient, 1).unwrap();
        let mut system = CapabilitySystem::try_new(8).unwrap();
        let root = system
            .insert_root(
                &mut source,
                &mut owner,
                object,
                ObjectType::Endpoint,
                Rights::READ,
                &arena,
            )
            .unwrap();
        let prepared = system
            .prepare_transfer(&source, root, Rights::READ, &arena)
            .unwrap();

        assert_eq!(
            system.commit_transfer(&source, prepared, &mut target, &mut recipient, &arena,),
            Err(CapabilityError::Resource(ResourceError::CapabilityLimit))
        );
        assert_eq!(recipient.usage().capabilities, 0);
        assert_eq!(
            system.lookup(&source, root, ObjectType::Endpoint, Rights::READ, &arena,),
            Ok(object)
        );
    }

    #[test]
    fn destroyed_objects_invalidate_existing_capabilities() {
        let mut owner = domain(DOMAIN_A, 2, 4);
        let mut arena = ObjectArena::try_new(2).unwrap();
        let object = endpoint(&mut arena, &mut owner);
        let mut space = CapabilitySpace::try_new(&owner, 2).unwrap();
        let mut system = CapabilitySystem::try_new(2).unwrap();
        let handle = system
            .insert_root(
                &mut space,
                &mut owner,
                object,
                ObjectType::Endpoint,
                Rights::READ,
                &arena,
            )
            .unwrap();

        arena.destroy(&mut owner, object).unwrap();
        assert_eq!(
            system.lookup(&space, handle, ObjectType::Endpoint, Rights::READ, &arena,),
            Err(CapabilityError::ObjectDestroyed)
        );
    }

    #[test]
    fn three_tier_cascade_revocation_across_spaces() {
        let mut domain_a = domain(DOMAIN_A, 2, 8);
        let mut domain_b = domain(DOMAIN_B, 2, 8);
        let mut domain_c = domain(ResourceDomainId::new(3), 2, 8);
        let mut arena = ObjectArena::try_new(2).unwrap();
        let object = endpoint(&mut arena, &mut domain_a);

        let mut space_a = CapabilitySpace::try_new(&domain_a, 4).unwrap();
        let mut space_b = CapabilitySpace::try_new(&domain_b, 4).unwrap();
        let mut space_c = CapabilitySpace::try_new(&domain_c, 4).unwrap();
        let mut system = CapabilitySystem::try_new(12).unwrap();

        let root = system
            .insert_root(
                &mut space_a,
                &mut domain_a,
                object,
                ObjectType::Endpoint,
                Rights::MANAGE | Rights::READ,
                &arena,
            )
            .unwrap();

        let child = system
            .derive(
                &space_a,
                root,
                &mut space_b,
                &mut domain_b,
                Rights::MANAGE | Rights::READ,
                &arena,
            )
            .unwrap();

        let grandchild = system
            .derive(
                &space_b,
                child,
                &mut space_c,
                &mut domain_c,
                Rights::READ,
                &arena,
            )
            .unwrap();

        // Revoke at intermediate level (space_b, child)
        system.revoke(&space_b, child, &arena).unwrap();

        // Root capability in space_a remains valid
        assert_eq!(
            system.lookup(&space_a, root, ObjectType::Endpoint, Rights::READ, &arena),
            Ok(object)
        );

        // Child in space_b and Grandchild in space_c are revoked
        assert_eq!(
            system.lookup(&space_b, child, ObjectType::Endpoint, Rights::READ, &arena),
            Err(CapabilityError::Revoked)
        );
        assert_eq!(
            system.lookup(
                &space_c,
                grandchild,
                ObjectType::Endpoint,
                Rights::READ,
                &arena
            ),
            Err(CapabilityError::Revoked)
        );
    }

    #[test]
    fn generational_handle_invalidation_prevents_stale_access() {
        let mut owner = domain(DOMAIN_A, 2, 8);
        let mut arena = ObjectArena::try_new(4).unwrap();
        let obj1 = endpoint(&mut arena, &mut owner);
        let obj2 = endpoint(&mut arena, &mut owner);

        let mut space = CapabilitySpace::try_new(&owner, 4).unwrap();
        let mut system = CapabilitySystem::try_new(8).unwrap();

        let old_handle = system
            .insert_root(
                &mut space,
                &mut owner,
                obj1,
                ObjectType::Endpoint,
                Rights::READ,
                &arena,
            )
            .unwrap();

        // Delete handle to free slot 0 and increment slot generation
        system.delete(&mut space, &mut owner, old_handle).unwrap();

        // Insert new capability into slot 0 with generation 2
        let new_handle = system
            .insert_root(
                &mut space,
                &mut owner,
                obj2,
                ObjectType::Endpoint,
                Rights::READ,
                &arena,
            )
            .unwrap();

        assert_ne!(old_handle, new_handle);
        assert_eq!(old_handle.slot(), new_handle.slot());

        // Old handle lookup fails with StaleHandle
        assert_eq!(
            system.lookup(
                &space,
                old_handle,
                ObjectType::Endpoint,
                Rights::READ,
                &arena
            ),
            Err(CapabilityError::StaleHandle)
        );

        // New handle lookup succeeds with obj2
        assert_eq!(
            system.lookup(
                &space,
                new_handle,
                ObjectType::Endpoint,
                Rights::READ,
                &arena
            ),
            Ok(obj2)
        );
    }

    #[test]
    fn rights_subset_matrix_is_exhaustive_for_all_initial_bits() {
        for parent_bits in 0_u32..(1 << 8) {
            let parent = Rights::from_bits(parent_bits);
            for requested_bits in 0_u32..(1 << 8) {
                let requested = Rights::from_bits(requested_bits);
                assert_eq!(
                    requested.is_subset_of(parent),
                    requested_bits & !parent.bits() == 0
                );
            }
        }
    }

    use crate::object::ObjectId;
    use crate::scheduler::{Scheduler, SchedulerError};
    use crate::thread::{Thread, ThreadState};

    fn test_object_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn thread_state_transitions() {
        let id = test_object_id(1);
        let mut thread = Thread::new(id, None, ());
        assert_eq!(thread.state(), ThreadState::New);

        // New -> Runnable
        assert_eq!(thread.make_runnable(), Ok(()));
        assert_eq!(thread.state(), ThreadState::Runnable);

        // Runnable -> Running
        assert_eq!(thread.make_running(), Ok(()));
        assert_eq!(thread.state(), ThreadState::Running);

        // Running -> Blocked
        assert_eq!(thread.make_blocked(), Ok(()));
        assert_eq!(thread.state(), ThreadState::Blocked);

        // Blocked -> Runnable
        assert_eq!(thread.make_runnable(), Ok(()));
        assert_eq!(thread.state(), ThreadState::Runnable);

        // Runnable -> Dying
        assert_eq!(thread.make_dying(), Ok(()));
        assert_eq!(thread.state(), ThreadState::Dying);

        // Dying -> Dead
        assert_eq!(thread.make_dead(), Ok(()));
        assert_eq!(thread.state(), ThreadState::Dead);

        // Dead -> Runnable (used for supervisor restart)
        assert_eq!(thread.make_runnable(), Ok(()));
        assert_eq!(thread.state(), ThreadState::Runnable);
    }

    #[test]
    fn scheduler_queue_logic() {
        let mut sched = Scheduler::try_new(2).unwrap();
        let mut t1 = Thread::new(test_object_id(1), None, ());
        let mut t2 = Thread::new(test_object_id(2), None, ());
        let mut t3 = Thread::new(test_object_id(3), None, ());

        assert_eq!(sched.enqueue(&mut t1), Ok(()));
        assert_eq!(sched.enqueue(&mut t2), Ok(()));
        assert_eq!(sched.enqueue(&mut t3), Err(SchedulerError::QueueFull));

        assert_eq!(sched.dequeue_next(), Some(t1.id()));
        assert_eq!(sched.dequeue_next(), Some(t2.id()));
        assert_eq!(sched.dequeue_next(), None);
    }

    #[test]
    fn factory_and_resourcedomain_correctness_tests() {
        let mut dom = domain(DOMAIN_A, 5, 5);
        let mut arena = ObjectArena::try_new(5).unwrap();

        // 1. Factory type enforcement denial
        let ep_only_factory = Factory::new(&dom, ObjectTypeSet::of(ObjectType::Endpoint));

        assert!(!ep_only_factory.allows(ObjectType::MemoryObject));
        assert!(ep_only_factory.allows(ObjectType::Endpoint));

        // Attempting to create MemoryObject via ep_only_factory fails with FactoryDenied, leaving state unchanged
        assert_eq!(
            arena.create(&mut dom, ep_only_factory, ObjectType::MemoryObject),
            Err(ObjectError::FactoryDenied)
        );
        assert_eq!(dom.usage().objects, 0);

        // 2. ResourceDomain byte quota enforcement
        assert_eq!(dom.usage().memory_bytes, 0);

        // Charge 32 KiB
        assert_eq!(dom.charge_memory(32768), Ok(()));
        assert_eq!(dom.usage().memory_bytes, 32768);

        // Charge exceeding remaining quota (65536 max limit)
        assert_eq!(dom.charge_memory(32769), Err(ResourceError::MemoryLimit));
        assert_eq!(dom.usage().memory_bytes, 32768); // State unchanged on error

        // Rollback memory
        assert_eq!(dom.rollback_memory(32768), Ok(()));
        assert_eq!(dom.usage().memory_bytes, 0);

        // 3. Narrow capability rights verification
        let mem_rights = Rights::MAP | Rights::READ | Rights::WRITE;
        assert!(!mem_rights.contains(Rights::EXECUTE));
        assert!(!mem_rights.contains(Rights::FACTORY));
        assert!(!mem_rights.contains(Rights::ALL));
        assert!(mem_rights.contains(Rights::MAP));
        assert!(mem_rights.contains(Rights::READ));
        assert!(mem_rights.contains(Rights::WRITE));
    }

    #[test]
    fn two_independent_delegations_selective_revocation_test() {
        let mut owner = domain(DOMAIN_A, 10, 20);
        let mut process_b_dom = domain(DOMAIN_B, 10, 20);
        let mut process_c_dom = domain(ResourceDomainId::new(3), 10, 20);

        let mut arena = ObjectArena::try_new(10).unwrap();
        let factory = Factory::new(&owner, ObjectTypeSet::of(ObjectType::MemoryObject));
        let mem_id = arena
            .create(&mut owner, factory, ObjectType::MemoryObject)
            .unwrap();

        let mut mem_obj = crate::memory::MemoryObject::new(mem_id, owner.id(), 65536);
        assert_eq!(mem_obj.total_refs(), 1); // 1 cap ref

        let mut space_a = CapabilitySpace::try_new(&owner, 8).unwrap();
        let mut space_a2 = CapabilitySpace::try_new(&owner, 8).unwrap();
        let mut space_b = CapabilitySpace::try_new(&process_b_dom, 8).unwrap();
        let mut space_c = CapabilitySpace::try_new(&process_c_dom, 8).unwrap();
        let mut system = CapabilitySystem::try_new(20).unwrap();

        // Process A creates root cap
        let root_cap = system
            .insert_root(
                &mut space_a,
                &mut owner,
                mem_id,
                ObjectType::MemoryObject,
                Rights::MAP | Rights::READ | Rights::WRITE | Rights::MANAGE,
                &arena,
            )
            .unwrap();

        // Process A derives branch 1 (cap_b1) into space_a2 and delegates to Process B (cap_b_target)
        let cap_b1 = system
            .derive(
                &space_a,
                root_cap,
                &mut space_a2,
                &mut owner,
                Rights::MAP | Rights::READ | Rights::WRITE | Rights::MANAGE,
                &arena,
            )
            .unwrap();
        let _ = mem_obj.inc_capability_ref();

        let cap_b_target = system
            .derive(
                &space_a2,
                cap_b1,
                &mut space_b,
                &mut process_b_dom,
                Rights::MAP | Rights::READ | Rights::WRITE,
                &arena,
            )
            .unwrap();
        let _ = mem_obj.inc_capability_ref();

        // Process A derives branch 2 (cap_c1) into space_a2 independently and delegates to Process C (cap_c_target)
        let cap_c1 = system
            .derive(
                &space_a,
                root_cap,
                &mut space_a2,
                &mut owner,
                Rights::MAP | Rights::READ | Rights::WRITE | Rights::MANAGE,
                &arena,
            )
            .unwrap();
        let _ = mem_obj.inc_capability_ref();

        let cap_c_target = system
            .derive(
                &space_a2,
                cap_c1,
                &mut space_c,
                &mut process_c_dom,
                Rights::MAP | Rights::READ | Rights::WRITE,
                &arena,
            )
            .unwrap();
        let _ = mem_obj.inc_capability_ref();

        assert_eq!(mem_obj.capability_refs(), 5); // root + b1 + b_target + c1 + c_target

        let node_b1 = system.node_for(&space_a2, cap_b1).unwrap();
        let node_c1 = system.node_for(&space_a2, cap_c1).unwrap();

        // Process B maps memory -> create mapping M_B with lineage = node_b1
        let mut mapping_b = crate::mapping::Mapping::try_new_memory_object(
            crate::object::ObjectId::new_for_test(100, 1),
            crate::object::ObjectId::new_for_test(200, 1),
            0x0000_6000_0000_0000,
            mem_id,
            0,
            65536,
            Rights::MAP | Rights::READ | Rights::WRITE,
            Some(node_b1),
        )
        .unwrap();
        let _ = mem_obj.inc_mapping_ref();

        // Process C maps memory -> create mapping M_C with lineage = node_c1
        let mapping_c = crate::mapping::Mapping::try_new_memory_object(
            crate::object::ObjectId::new_for_test(101, 1),
            crate::object::ObjectId::new_for_test(201, 1),
            0x0000_7000_0000_0000,
            mem_id,
            0,
            65536,
            Rights::MAP | Rights::READ | Rights::WRITE,
            Some(node_c1),
        )
        .unwrap();
        let _ = mem_obj.inc_mapping_ref();

        assert_eq!(mem_obj.mapping_refs(), 2);
        assert_eq!(mem_obj.total_refs(), 7);

        // Revoke branch 1 (cap_b1)
        system.revoke(&space_a2, cap_b1, &arena).unwrap();

        // Verify cap_b1 and cap_b_target are revoked
        assert_eq!(
            system.lookup(
                &space_a2,
                cap_b1,
                ObjectType::MemoryObject,
                Rights::READ,
                &arena
            ),
            Err(CapabilityError::Revoked)
        );
        assert_eq!(
            system.lookup(
                &space_b,
                cap_b_target,
                ObjectType::MemoryObject,
                Rights::READ,
                &arena
            ),
            Err(CapabilityError::Revoked)
        );

        // Verify cap_c1 and cap_c_target remain 100% LIVE
        assert_eq!(
            system.lookup(
                &space_a2,
                cap_c1,
                ObjectType::MemoryObject,
                Rights::READ,
                &arena
            ),
            Ok(mem_id)
        );
        assert_eq!(
            system.lookup(
                &space_c,
                cap_c_target,
                ObjectType::MemoryObject,
                Rights::READ,
                &arena
            ),
            Ok(mem_id)
        );

        // Selective lineage unmapping: M_B is descendant of node_b1 -> close M_B
        assert!(system.is_descendant_of(mapping_b.lineage_parent_node().unwrap(), node_b1));
        assert!(!system.is_descendant_of(mapping_c.lineage_parent_node().unwrap(), node_b1));

        mapping_b.close().unwrap();
        mem_obj.dec_mapping_ref().unwrap();
        mem_obj.dec_capability_ref().unwrap(); // cap_b1
        mem_obj.dec_capability_ref().unwrap(); // cap_b_target

        // Memory object is NOT destroyed because Process C mapping & capabilities remain live!
        assert!(!mem_obj.can_destroy());
        assert_eq!(mem_obj.mapping_refs(), 1); // Process C mapping still active

        // Now revoke branch 2 (cap_c1)
        system.revoke(&space_a2, cap_c1, &arena).unwrap();
        mem_obj.dec_mapping_ref().unwrap();
        mem_obj.dec_capability_ref().unwrap(); // cap_c1
        // 5. Release root_cap and initial MemoryObject capability references -> total_refs reaches 0 -> reclaim & refund
        mem_obj.dec_capability_ref().unwrap(); // root_cap
        let can_destroy = mem_obj.dec_capability_ref().unwrap(); // initial creation ref

        assert!(can_destroy);
        assert!(mem_obj.can_destroy());
    }
}
