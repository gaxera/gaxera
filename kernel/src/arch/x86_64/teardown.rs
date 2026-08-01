use crate::global::{
    ADDRESS_SPACES, CAPABILITY_SPACES, CAPABILITY_SYSTEM, MAPPINGS, MEMORY_OBJECTS, OBJECT_ARENA,
    RESOURCE_DOMAINS,
};
use gaxera_abi::{Handle, ObjectType, Rights};
use kernel_core::object::ObjectId;
use kernel_core::registry::ObjectRegistry;

#[allow(clippy::result_unit_err)]
pub fn delete_handle_internal(cspace_id: ObjectId, target_handle: Handle) -> Result<(), ()> {
    let caller_domain_id = {
        let cspaces = CAPABILITY_SPACES.lock();
        cspaces.get(cspace_id).ok_or(())?.domain()
    };
    let mut domains = RESOURCE_DOMAINS.lock();
    let domain_guard = domains
        .iter_mut()
        .find(|d| d.id() == caller_domain_id)
        .ok_or(())?;
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
    let is_mapping = sys.lookup(
        cspace,
        target_handle,
        ObjectType::Mapping,
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
    } else if let Ok(mapping_id) = is_mapping {
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
                && let Some(domain) = domains.iter_mut().find(|d| d.id() == owner_id)
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
            }
        }
    }

    Ok(())
}
