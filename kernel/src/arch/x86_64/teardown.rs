#![allow(clippy::undocumented_unsafe_blocks, clippy::collapsible_if)]

use crate::global::{
    ADDRESS_SPACES, CAPABILITY_SPACES, CAPABILITY_SYSTEM, CONTIGUOUS_FRAMES, ENDPOINTS, INTERRUPTS,
    MAPPINGS, MEMORY_OBJECTS, NOTIFICATIONS, OBJECT_ARENA, RESOURCE_DOMAINS, WAIT_SETS,
};
use gaxera_abi::{Handle, ObjectType, Rights};
use kernel_core::object::ObjectId;
use kernel_core::registry::ObjectRegistry;

fn wake_thread(thread_id: ObjectId) {
    // SAFETY: Thread access and scheduler operations are synchronized under single-CPU execution.
    unsafe {
        if let Some(thread) = crate::arch::x86_64::thread::THREADS.get_mut(thread_id) {
            let cpu_local = crate::arch::x86_64::cpu::get_cpu_local();
            let scheduler = &mut *cpu_local.scheduler.get();
            if let Some(s) = scheduler.as_mut() {
                let _ = s.apply_wake(thread);
            }
        }
    }
}

#[allow(clippy::result_unit_err)]
pub fn delete_handle_internal(cspace_id: ObjectId, target_handle: Handle) -> Result<(), ()> {
    let caller_domain_id = {
        let cspaces = CAPABILITY_SPACES.lock();
        cspaces.get(cspace_id).ok_or(())?.domain()
    };
    let mut domains = RESOURCE_DOMAINS.lock();
    let domain_guard = domains.get_mut(caller_domain_id.object_id()).ok_or(())?;
    let mut system = CAPABILITY_SYSTEM.lock();
    let mut arena = OBJECT_ARENA.lock();
    let mut cspaces = CAPABILITY_SPACES.lock();

    let cspace = cspaces.get_mut(cspace_id).ok_or(())?;
    let sys = system.as_mut().ok_or(())?;
    let arena_ref = arena.as_mut().ok_or(())?;

    let is_mem_obj = sys.lookup(
        cspace,
        target_handle,
        ObjectType::MemoryObject,
        Rights::NONE,
        arena_ref,
    );
    let is_contiguous_frame = sys.lookup(
        cspace,
        target_handle,
        ObjectType::ContiguousFrame,
        Rights::NONE,
        arena_ref,
    );
    let is_mapping = sys.lookup(
        cspace,
        target_handle,
        ObjectType::Mapping,
        Rights::NONE,
        arena_ref,
    );
    let is_notif = sys.lookup(
        cspace,
        target_handle,
        ObjectType::Notification,
        Rights::NONE,
        arena_ref,
    );
    let is_endpoint = sys.lookup(
        cspace,
        target_handle,
        ObjectType::Endpoint,
        Rights::NONE,
        arena_ref,
    );
    let is_waitset = sys.lookup(
        cspace,
        target_handle,
        ObjectType::WaitSet,
        Rights::NONE,
        arena_ref,
    );
    let is_interrupt = sys.lookup(
        cspace,
        target_handle,
        ObjectType::InterruptObject,
        Rights::NONE,
        arena_ref,
    );

    let delete_res = sys.delete(cspace, domain_guard, target_handle);
    drop(cspaces);
    drop(arena);
    drop(system);
    drop(domains);

    if delete_res.is_err() {
        return Err(());
    }

    if let Ok(mem_id) = is_mem_obj {
        let mut mem_objects = MEMORY_OBJECTS.lock();
        let can_destroy = if let Some(mem_obj) = mem_objects.get_mut(mem_id) {
            mem_obj.dec_capability_ref().unwrap_or(false)
        } else {
            false
        };
        drop(mem_objects);
        if can_destroy {
            super::syscall::reclaim_memory_object_if_zero_refs(mem_id);
        }
    } else if let Ok(frame_id) = is_contiguous_frame {
        let can_destroy = {
            let mut frames = CONTIGUOUS_FRAMES.lock();
            frames
                .get_mut(frame_id)
                .and_then(|frame| frame.dec_capability().ok())
                .unwrap_or(false)
        };
        if can_destroy {
            super::syscall::reclaim_contiguous_frame_if_zero_refs(frame_id);
        }
    } else if let Ok(mapping_id) = is_mapping {
        let last_capability = {
            let mut mappings = MAPPINGS.lock();
            mappings
                .get_mut(mapping_id)
                .and_then(|mapping| mapping.dec_capability_ref().ok())
                .unwrap_or(false)
        };
        if !last_capability {
            return Ok(());
        }
        let mappings = MAPPINGS.lock();
        if let Some(m) = mappings.get(mapping_id) {
            let vaddr = m.virtual_address();
            let size = m.size();
            let aspace_id = m.target_address_space();
            let backing = m.backing().clone();
            drop(mappings);

            let mut aspaces = ADDRESS_SPACES.lock();
            if let Some(aspace) = aspaces.get_mut(aspace_id) {
                use kernel_core::address_space::ArchAddressSpace;
                let _ = aspace.arch.unmap_range(vaddr, size / 4096);
            }
            drop(aspaces);

            let mut flush_vaddr = vaddr;
            let end_vaddr = vaddr + size as u64;
            while flush_vaddr < end_vaddr {
                x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(flush_vaddr));
                flush_vaddr += 4096;
            }

            let mut mappings = MAPPINGS.lock();
            if let Some(m) = mappings.get_mut(mapping_id) {
                let _ = m.close();
            }
            let _ = mappings.remove(mapping_id);
            drop(mappings);

            let mut domains = RESOURCE_DOMAINS.lock();
            let mut arena_guard = OBJECT_ARENA.lock();
            if let Some(arena) = arena_guard.as_mut()
                && let Some(owner_id) = arena.owner(mapping_id)
                && let Some(domain) = domains.get_mut(owner_id.object_id())
            {
                let _ = arena.destroy(domain, mapping_id);
            }
            drop(arena_guard);
            drop(domains);

            if let kernel_core::mapping::MappingBacking::MemoryObject { object_id, .. } = backing {
                let mut mem_objects = MEMORY_OBJECTS.lock();
                let can_destroy = if let Some(mem_obj) = mem_objects.get_mut(object_id) {
                    mem_obj.dec_mapping_ref().unwrap_or(false)
                } else {
                    false
                };
                drop(mem_objects);
                if can_destroy {
                    super::syscall::reclaim_memory_object_if_zero_refs(object_id);
                }
            } else if let kernel_core::mapping::MappingBacking::ContiguousFrame {
                object_id, ..
            } = backing
            {
                let can_destroy = {
                    let mut frames = CONTIGUOUS_FRAMES.lock();
                    frames.get_mut(object_id).map(|frame| {
                        if frame.remove_mapping().is_ok() {
                            frame.can_destroy()
                        } else {
                            false
                        }
                    })
                }
                .unwrap_or(false);
                if can_destroy {
                    super::syscall::reclaim_contiguous_frame_if_zero_refs(object_id);
                }
            }
        }
    } else if let Ok(notif_id) = is_notif {
        let last_capability = {
            let mut notifications = NOTIFICATIONS.lock();
            notifications
                .get_mut(notif_id)
                .and_then(|notification| notification.dec_capability_ref().ok())
                .unwrap_or(false)
        };
        if !last_capability {
            return Ok(());
        }
        let mut notifications = NOTIFICATIONS.lock();
        if let Some(notif) = notifications.get_mut(notif_id) {
            if let Some(waiting_thread_id) = notif.close() {
                wake_thread(waiting_thread_id);
            }
        }
    } else if let Ok(endpoint_id) = is_endpoint {
        let mut endpoints = ENDPOINTS.lock();
        if let Some(endpoint) = endpoints.get_mut(endpoint_id) {
            let effects = endpoint.close();
            for thread_id in effects.woke_threads {
                wake_thread(thread_id);
            }
        }
    } else if let Ok(waitset_id) = is_waitset {
        let mut waitsets = WAIT_SETS.lock();
        if let Some(ws) = waitsets.get_mut(waitset_id) {
            if let Some(waiting_thread_id) = ws.close() {
                wake_thread(waiting_thread_id);
            }
        }
    } else if let Ok(irq_id) = is_interrupt {
        let last_capability = {
            let mut interrupts = INTERRUPTS.lock();
            interrupts
                .get_mut(irq_id)
                .and_then(|interrupt| interrupt.dec_capability_ref().ok())
                .unwrap_or(false)
        };
        // Capture the lease before closing the typed object.  The vector
        // generation must be validated during release so a stale teardown
        // cannot release a later owner's vector.
        let lease = {
            let interrupts = INTERRUPTS.lock();
            interrupts.get(irq_id).map(|irq| {
                crate::arch::x86_64::interrupts::VectorLease::from_parts(
                    irq.vector(),
                    irq.generation(),
                )
            })
        };

        if let Some(irq) = lease.and_then(|lease| {
            let interrupts = INTERRUPTS.lock();
            interrupts.get(irq_id).map(|irq| (lease, irq.irq()))
        }) {
            // Mask the controller before publishing the vector as free.  A
            // late device edge then observes an inactive vector and cannot
            // signal a replacement capability.
            crate::arch::x86_64::ioapic::ioapic_mask_irq(irq.1);
            let _ = crate::arch::x86_64::interrupts::unbind(irq.0);
            if last_capability {
                let _ = crate::arch::x86_64::interrupts::release(irq.0);
            }
        }

        if !last_capability {
            if let Some(irq) = INTERRUPTS.lock().get_mut(irq_id) {
                let _ = irq.unbind_notification();
            }
            return Ok(());
        }

        {
            let mut interrupts = INTERRUPTS.lock();
            if let Some(irq) = interrupts.get_mut(irq_id) {
                irq.close();
            }
            let _ = interrupts.remove(irq_id);
        }

        // `CapabilitySystem::delete` removes only the capability node.  The
        // arena entry is destroyed separately, after all rank-4 registry
        // locks have been released.
        let mut domains = RESOURCE_DOMAINS.lock();
        let mut arena_guard = OBJECT_ARENA.lock();
        if let Some(arena) = arena_guard.as_mut()
            && let Some(owner_id) = arena.owner(irq_id)
            && let Some(domain) = domains.get_mut(owner_id.object_id())
        {
            let _ = arena.destroy(domain, irq_id);
        }
    }

    Ok(())
}
