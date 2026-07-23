#![no_std]

extern crate alloc;

pub mod address_space;

pub mod capability;
pub mod debug_console;
pub mod elf;
pub mod ipc;
pub mod memory;
pub mod notification;
pub mod object;
pub mod registry;
pub mod resource;
pub mod scheduler;
pub mod slab;
pub mod thread;
pub mod time;
pub mod timer;

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
}
