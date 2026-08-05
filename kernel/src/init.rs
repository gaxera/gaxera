use crate::arch::x86_64::paging::KernelPageTables;
use crate::memory::boot::BootContext;
use alloc::vec::Vec;
use gaxera_abi::ObjectTypeSet;
use kernel_core::elf::error::ElfError;
use kernel_core::elf::parser::ElfParser;
use kernel_core::object::{Factory, ObjectArena, ObjectError, ResourceDomain};
use x86_64::structures::paging::{
    FrameAllocator, FrameDeallocator, PageSize, PageTableFlags, Size4KiB,
};

#[derive(Clone, Copy, Debug)]
pub enum InitError {
    ModuleNotFound,
    ElfParse(ElfError),
    Object(ObjectError),
    StackAllocationFailed,
}

impl From<ObjectError> for InitError {
    fn from(err: ObjectError) -> Self {
        Self::Object(err)
    }
}

impl From<ElfError> for InitError {
    fn from(err: ElfError) -> Self {
        Self::ElfParse(err)
    }
}

#[allow(unused_variables)]
pub fn spawn_init(
    boot_context: &'static BootContext,
    page_tables: &mut KernelPageTables,
    mut arena: ObjectArena,
    mut system: kernel_core::capability::CapabilitySystem,
    virtio_rng_info: Option<crate::arch::x86_64::pci::VirtioRngPciInfo>,
) -> Result<!, InitError> {
    let mut phys_alloc_guard = crate::global::PHYSICAL_ALLOCATOR.lock();
    let physical_allocator = phys_alloc_guard
        .as_deref_mut()
        .ok_or(InitError::StackAllocationFailed)?;
    let init_module = boot_context
        .find_module("init")
        .ok_or(InitError::ModuleNotFound)?;

    crate::println!(
        "GAXERA: INIT_MODULE found at phys={:#018x} size={}",
        init_module.physical_address,
        init_module.size
    );

    // SAFETY: Hardware invariant or verified by caller.
    let module_data = unsafe {
        core::slice::from_raw_parts(
            (init_module.physical_address + crate::memory::mapping::HHDM_BASE) as *const u8,
            init_module.size as usize,
        )
    };

    let parser = ElfParser::new(module_data)?;

    // Bootstrap Capability Graph
    let domain_id = kernel_core::object::ResourceDomainId::new_for_test(1);
    let limits = kernel_core::resource::ResourceLimits {
        objects: 1024,
        capabilities: 1024,
        memory_bytes: 64 * 1024 * 1024,
    };
    let mut domain = ResourceDomain::new_for_test(domain_id, limits); // 1 = Domain 1 for init

    // Create a Factory capability that can produce all ObjectTypes
    let factory = Factory::new_root(&domain, ObjectTypeSet::ALL);

    let aspace_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::AddressSpace)?;
    let cspace_id = arena.create(
        &mut domain,
        factory,
        gaxera_abi::ObjectType::CapabilitySpace,
    )?;
    let thread_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::Thread)?;
    let factory_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::Factory)?;
    let image_factory = Factory::new_root(
        &domain,
        ObjectTypeSet::of(gaxera_abi::ObjectType::MemoryObject),
    );
    let image_factory_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::Factory)?;
    let init_process_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::Process)?;
    let idle_thread_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::Thread)?;
    #[cfg(any(feature = "test-irq-notification", feature = "test-virtio-rng"))]
    let interrupt_id = arena.create(
        &mut domain,
        factory,
        gaxera_abi::ObjectType::InterruptObject,
    )?;
    crate::global::FACTORIES.lock().insert(factory_id, factory);
    crate::global::FACTORIES
        .lock()
        .insert(image_factory_id, image_factory);
    #[cfg(any(feature = "test-irq-notification", feature = "test-virtio-rng"))]
    {
        #[cfg(feature = "test-irq-notification")]
        let interrupt_irq = 1;
        #[cfg(feature = "test-virtio-rng")]
        let interrupt_irq = virtio_rng_info
            .ok_or(InitError::StackAllocationFailed)?
            .interrupt_line;
        let lease = crate::arch::x86_64::interrupts::allocate(interrupt_irq, interrupt_id)
            .map_err(|_| InitError::StackAllocationFailed)?;
        #[cfg(feature = "test-virtio-rng")]
        crate::arch::x86_64::ioapic::ioapic_set_redirection_with_trigger(
            interrupt_irq,
            lease.vector(),
            0,
            true,
            true,
            true,
        );
        #[cfg(feature = "test-irq-notification")]
        crate::arch::x86_64::ioapic::ioapic_set_redirection(interrupt_irq, lease.vector(), 0, true);
        crate::global::INTERRUPTS.lock().insert(
            interrupt_id,
            kernel_core::interrupt::InterruptObject::with_metadata(
                interrupt_id,
                lease.vector(),
                interrupt_irq,
                lease.generation(),
                kernel_core::interrupt::InterruptTrigger::Level,
                None,
            ),
        );
    }

    // Allocate Kernel Stack for init syscalls
    let kernel_stack =
        crate::arch::x86_64::stack::KernelStack::allocate(page_tables, physical_allocator)
            .map_err(|_| InitError::StackAllocationFailed)?;
    let kernel_stack_top = kernel_stack.top().as_u64();
    crate::println!("GAXERA: KERNEL_STACK top={:#018x}", kernel_stack_top);
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        crate::arch::x86_64::cpu::set_kernel_stack_top(kernel_stack_top);
    }

    // Create X86AddressSpace after KernelStack is allocated so it inherits the stack mapping in the upper half.
    let x86_aspace =
        crate::arch::x86_64::address_space::X86AddressSpace::new(page_tables, physical_allocator)
            .map_err(|_| InitError::StackAllocationFailed)?;

    use kernel_core::address_space::ArchAddressSpace;
    let init_pml4 = x86_aspace.root_token();

    // Map ELF segments into the isolated AddressSpace
    use crate::memory::mapping::HHDM_BASE;
    use crate::memory::physical::PAGE_SIZE;
    use kernel_core::elf::types::{PF_W, PF_X, PT_LOAD};

    for segment in parser.program_headers() {
        if segment.p_type != PT_LOAD {
            continue;
        }

        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        let is_writable = (segment.p_flags & PF_W) != 0;
        let is_executable = (segment.p_flags & PF_X) != 0;

        if is_writable {
            flags |= PageTableFlags::WRITABLE;
        }
        if !is_executable {
            flags |= PageTableFlags::NO_EXECUTE;
        }

        let start_page = segment.p_vaddr & !(PAGE_SIZE - 1);
        let end_vaddr = segment.p_vaddr.checked_add(segment.p_memsz).unwrap();
        let end_page = (end_vaddr + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

        let mut current_vaddr = start_page;

        while current_vaddr < end_page {
            let phys_frame = match physical_allocator.allocate_frame() {
                Some(frame) => frame,
                None => return Err(InitError::StackAllocationFailed),
            };

            // SAFETY: Verified isolated address space.
            if unsafe {
                KernelPageTables::map_user_page_in_pml4(
                    init_pml4,
                    current_vaddr,
                    phys_frame,
                    flags,
                    physical_allocator,
                )
            }
            .is_err()
            {
                // SAFETY: The failed map did not publish the frame.
                unsafe { physical_allocator.deallocate_frame(phys_frame) };
                return Err(InitError::StackAllocationFailed);
            }

            let frame_ptr = (HHDM_BASE + phys_frame.start_address().as_u64()) as *mut u8;
            // SAFETY: Hardware invariant.
            unsafe {
                core::ptr::write_bytes(frame_ptr, 0, PAGE_SIZE as usize);
            }

            if current_vaddr < segment.p_vaddr + segment.p_filesz
                && current_vaddr + PAGE_SIZE > segment.p_vaddr
            {
                let overlap_start_vaddr = core::cmp::max(current_vaddr, segment.p_vaddr);
                let overlap_end_vaddr = core::cmp::min(
                    current_vaddr + PAGE_SIZE,
                    segment.p_vaddr + segment.p_filesz,
                );
                let overlap_len = (overlap_end_vaddr - overlap_start_vaddr) as usize;

                let file_offset = segment.p_offset + (overlap_start_vaddr - segment.p_vaddr);
                let dest_offset = overlap_start_vaddr - current_vaddr;

                // SAFETY: Valid pointer arithmetic inside bootloader module bounds.
                let src_ptr = unsafe { module_data.as_ptr().add(file_offset as usize) };
                // SAFETY: Valid pointer arithmetic inside allocated frame bounds.
                let dest_ptr = unsafe { frame_ptr.add(dest_offset as usize) };

                // SAFETY: Lengths are bounded by the overlapping pages.
                unsafe {
                    core::ptr::copy_nonoverlapping(src_ptr, dest_ptr, overlap_len);
                }
            }

            current_vaddr += PAGE_SIZE;
        }
    }

    // Allocate a bounded user entry stack with one unmapped guard page below
    // it. A single page is insufficient for the Rust entry prologue and the
    // allocator/bootstrap work performed before init reaches its main loop.
    const USER_STACK_PAGES: u64 = 16;
    let user_stack_top = 0x0000_8000_0000_0000u64;
    let user_stack_vaddr = user_stack_top
        .checked_sub(USER_STACK_PAGES * Size4KiB::SIZE)
        .ok_or(InitError::StackAllocationFailed)?;
    let mut user_stack_frames = Vec::new();
    user_stack_frames
        .try_reserve_exact(USER_STACK_PAGES as usize)
        .map_err(|_| InitError::StackAllocationFailed)?;
    crate::println!(
        "GAXERA: USER_STACK base={:#018x} top={:#018x} pages={}",
        user_stack_vaddr,
        user_stack_top,
        USER_STACK_PAGES
    );

    for page_index in 0..USER_STACK_PAGES {
        let frame = match physical_allocator.allocate_frame() {
            Some(frame) => frame,
            None => {
                for (mapped_index, mapped_frame) in user_stack_frames.iter().copied().enumerate() {
                    // SAFETY: Only pages mapped by this unpublished bootstrap
                    // transaction are being removed.
                    let _ = unsafe {
                        KernelPageTables::unmap_user_range(
                            init_pml4,
                            user_stack_vaddr + mapped_index as u64 * Size4KiB::SIZE,
                            1,
                        )
                    };
                    // SAFETY: The frame is no longer mapped and remains
                    // exclusively owned by this rollback path.
                    unsafe { physical_allocator.deallocate_frame(mapped_frame) };
                }
                return Err(InitError::StackAllocationFailed);
            }
        };
        let page_vaddr = user_stack_vaddr + page_index * Size4KiB::SIZE;
        // SAFETY: The HHDM gives a valid writable alias for the newly
        // allocated frame; clearing it prevents stale kernel data exposure.
        unsafe {
            core::ptr::write_bytes(
                (crate::memory::mapping::HHDM_BASE + frame.start_address().as_u64()) as *mut u8,
                0,
                Size4KiB::SIZE as usize,
            );
        }
        // SAFETY: `frame` is exclusively owned and `page_vaddr` is inside the
        // private stack range in the new address space.
        if unsafe {
            KernelPageTables::map_user_page_in_pml4(
                init_pml4,
                page_vaddr,
                frame,
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::USER_ACCESSIBLE
                    | PageTableFlags::NO_EXECUTE,
                physical_allocator,
            )
        }
        .is_err()
        {
            // SAFETY: No mapping was published for `frame` on this error.
            unsafe { physical_allocator.deallocate_frame(frame) };
            for (mapped_index, mapped_frame) in user_stack_frames.iter().copied().enumerate() {
                // SAFETY: Only pages mapped by this unpublished bootstrap
                // transaction are being removed.
                let _ = unsafe {
                    KernelPageTables::unmap_user_range(
                        init_pml4,
                        user_stack_vaddr + mapped_index as u64 * Size4KiB::SIZE,
                        1,
                    )
                };
                // SAFETY: The frame is no longer mapped and remains
                // exclusively owned by this rollback path.
                unsafe { physical_allocator.deallocate_frame(mapped_frame) };
            }
            return Err(InitError::StackAllocationFailed);
        }
        user_stack_frames.push(frame);
    }

    // Keep the entry stack private and writable, but place the bootstrap
    // manifest in a separate user-readable, non-writable page. The manifest
    // is kernel-authored input, not process-owned mutable state.
    let page_size = Size4KiB::SIZE;
    let hhdm_stack_top = crate::memory::mapping::HHDM_BASE
        + user_stack_frames
            .last()
            .ok_or(InitError::StackAllocationFailed)?
            .start_address()
            .as_u64()
        + page_size;
    let boot_info_size = core::mem::size_of::<gaxera_abi::boot::BootInfo>() as u64;
    let boot_info_addr_hhdm = hhdm_stack_top - boot_info_size;
    let boot_info = gaxera_abi::boot::BootInfo {
        magic: gaxera_abi::boot::BootInfo::MAGIC,
        abi_version: gaxera_abi::boot::BootInfo::ABI_VERSION,
        reserved: 0,
    };

    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        core::ptr::write(
            boot_info_addr_hhdm as *mut gaxera_abi::boot::BootInfo,
            boot_info,
        );
    }

    let manifest_frame = physical_allocator
        .allocate_frame()
        .ok_or(InitError::StackAllocationFailed)?;
    let user_manifest_ptr = gaxera_abi::boot::BOOTSTRAP_MANIFEST_VADDR;
    let manifest_flags =
        PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE;
    // SAFETY: `manifest_frame` is exclusively owned by this bootstrap path,
    // and the fixed virtual address is outside the entry stack page.
    if unsafe {
        KernelPageTables::map_user_page_in_pml4(
            init_pml4,
            user_manifest_ptr,
            manifest_frame,
            manifest_flags,
            physical_allocator,
        )
    }
    .is_err()
    {
        // SAFETY: The mapping failed, so ownership of `manifest_frame` never
        // escaped this bootstrap transaction.
        unsafe { physical_allocator.deallocate_frame(manifest_frame) };
        return Err(InitError::StackAllocationFailed);
    }
    let manifest_addr_hhdm =
        crate::memory::mapping::HHDM_BASE + manifest_frame.start_address().as_u64();
    let user_rsp = user_stack_top - 16; // 16-byte align stack

    let entry_point = parser.header().e_entry;
    crate::println!(
        "GAXERA: ENTER_USER_MODE entry={:#018x} rsp={:#018x} manifest={:#018x}",
        entry_point,
        user_rsp,
        user_manifest_ptr
    );

    let aspace = kernel_core::address_space::AddressSpace::new(aspace_id, x86_aspace);
    let mut cspace = kernel_core::capability::CapabilitySpace::try_new(&domain, 1024)
        .map_err(|_| InitError::StackAllocationFailed)?;

    let mut bootstrap_entries = [gaxera_abi::boot::BootstrapCapability {
        role: 0,
        object_type: 0,
        flags: 0,
        rights: 0,
        handle: gaxera_abi::Handle::INVALID,
        metadata: 0,
    }; gaxera_abi::boot::MAX_BOOTSTRAP_CAPABILITIES];
    let mut bootstrap_count = 0usize;

    // Insert initial capabilities and record their actual opaque handles.
    let h_aspace = system
        .insert_root(
            &mut cspace,
            &mut domain,
            aspace_id,
            gaxera_abi::ObjectType::AddressSpace,
            gaxera_abi::Rights::MAP,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    let h_cspace = system
        .insert_root(
            &mut cspace,
            &mut domain,
            cspace_id,
            gaxera_abi::ObjectType::CapabilitySpace,
            gaxera_abi::Rights::MANAGE,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    let h_thread = system
        .insert_root(
            &mut cspace,
            &mut domain,
            thread_id,
            gaxera_abi::ObjectType::Thread,
            gaxera_abi::Rights::MANAGE,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    let h_factory = system
        .insert_root(
            &mut cspace,
            &mut domain,
            factory_id,
            gaxera_abi::ObjectType::Factory,
            gaxera_abi::Rights::FACTORY,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    let h_image_factory = system
        .insert_root(
            &mut cspace,
            &mut domain,
            image_factory_id,
            gaxera_abi::ObjectType::Factory,
            gaxera_abi::Rights::FACTORY | gaxera_abi::Rights::IMAGE_FACTORY,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    #[cfg(any(feature = "test-irq-notification", feature = "test-virtio-rng"))]
    let h_interrupt = system
        .insert_root(
            &mut cspace,
            &mut domain,
            interrupt_id,
            gaxera_abi::ObjectType::InterruptObject,
            gaxera_abi::Rights::INTERRUPT,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;

    #[allow(unused_mut)]
    let mut device_mappings = Vec::new();
    #[cfg(feature = "test-virtio-rng")]
    {
        let info = virtio_rng_info.ok_or(InitError::StackAllocationFailed)?;
        for region in [info.common, info.notify, info.isr] {
            let mapping_id = arena.create_mapping(&mut domain)?;
            let region_physical_base = region
                .window
                .physical_base
                .checked_add(u64::from(region.offset))
                .ok_or(InitError::StackAllocationFailed)?;
            let region_size = u64::from(region.length)
                .checked_add(crate::memory::physical::PAGE_SIZE - 1)
                .ok_or(InitError::StackAllocationFailed)?
                & !(crate::memory::physical::PAGE_SIZE - 1);
            let mapping = kernel_core::mapping::Mapping::try_new_mmio(
                mapping_id,
                aspace_id,
                0,
                region_physical_base,
                usize::try_from(region_size).map_err(|_| InitError::StackAllocationFailed)?,
                gaxera_abi::CachePolicy::Uncached,
                gaxera_abi::Rights::MAP | gaxera_abi::Rights::READ | gaxera_abi::Rights::WRITE,
            )
            .map_err(|_| InitError::StackAllocationFailed)?;
            let handle = system
                .insert_root(
                    &mut cspace,
                    &mut domain,
                    mapping_id,
                    gaxera_abi::ObjectType::Mapping,
                    gaxera_abi::Rights::MAP | gaxera_abi::Rights::READ | gaxera_abi::Rights::WRITE,
                    &arena,
                )
                .map_err(|_| InitError::StackAllocationFailed)?;
            let metadata = u64::from(region.cfg_type)
                | (u64::from(region.bar) << 8)
                | (u64::from(region.notify_off_multiplier) << 48);
            if bootstrap_count == gaxera_abi::boot::MAX_BOOTSTRAP_CAPABILITIES {
                return Err(InitError::StackAllocationFailed);
            }
            bootstrap_entries[bootstrap_count] = gaxera_abi::boot::BootstrapCapability {
                role: gaxera_abi::boot::BootstrapRole::DeviceMemory as u16,
                object_type: gaxera_abi::ObjectType::Mapping as u8,
                flags: 0,
                rights: (gaxera_abi::Rights::MAP
                    | gaxera_abi::Rights::READ
                    | gaxera_abi::Rights::WRITE)
                    .bits(),
                handle,
                metadata,
            };
            bootstrap_count += 1;
            device_mappings.push((mapping_id, mapping));
        }
    }

    for (role, object_type, rights, handle) in [
        (
            gaxera_abi::boot::BootstrapRole::SelfAddressSpace,
            gaxera_abi::ObjectType::AddressSpace,
            gaxera_abi::Rights::MAP,
            h_aspace,
        ),
        (
            gaxera_abi::boot::BootstrapRole::SelfThread,
            gaxera_abi::ObjectType::Thread,
            gaxera_abi::Rights::MANAGE,
            h_thread,
        ),
        (
            gaxera_abi::boot::BootstrapRole::HeapFactory,
            gaxera_abi::ObjectType::Factory,
            gaxera_abi::Rights::FACTORY,
            h_factory,
        ),
        (
            gaxera_abi::boot::BootstrapRole::SelfCapabilitySpace,
            gaxera_abi::ObjectType::CapabilitySpace,
            gaxera_abi::Rights::MANAGE,
            h_cspace,
        ),
        (
            gaxera_abi::boot::BootstrapRole::ImageFactory,
            gaxera_abi::ObjectType::Factory,
            gaxera_abi::Rights::FACTORY | gaxera_abi::Rights::IMAGE_FACTORY,
            h_image_factory,
        ),
    ] {
        bootstrap_entries[bootstrap_count] = gaxera_abi::boot::BootstrapCapability {
            role: role as u16,
            object_type: object_type as u8,
            flags: 0,
            rights: rights.bits(),
            handle,
            metadata: 1,
        };
        bootstrap_count += 1;
    }

    #[cfg(any(feature = "test-irq-notification", feature = "test-virtio-rng"))]
    {
        bootstrap_entries[bootstrap_count] = gaxera_abi::boot::BootstrapCapability {
            role: gaxera_abi::boot::BootstrapRole::InterruptObject as u16,
            object_type: gaxera_abi::ObjectType::InterruptObject as u8,
            flags: 0,
            rights: gaxera_abi::Rights::INTERRUPT.bits(),
            handle: h_interrupt,
            metadata: {
                #[cfg(feature = "test-irq-notification")]
                {
                    1
                }
                #[cfg(feature = "test-virtio-rng")]
                {
                    virtio_rng_info
                        .ok_or(InitError::StackAllocationFailed)?
                        .interrupt_line as u64
                }
            },
        };
        bootstrap_count += 1;
    }

    // Insert boot modules as MemoryObjects (Handles 4+)
    for module in boot_context.boot_modules() {
        let mem_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::MemoryObject)?;
        let mapped_size = module
            .size
            .checked_add(crate::memory::physical::PAGE_SIZE - 1)
            .ok_or(InitError::StackAllocationFailed)?
            & !(crate::memory::physical::PAGE_SIZE - 1);
        let mut mem_obj = kernel_core::memory::MemoryObject::new(mem_id, domain.id(), mapped_size);

        let start_frame = module.physical_address & !0xFFF;
        let end_frame = (module.physical_address + module.size + 0xFFF) & !0xFFF;
        let mut frame_addr = start_frame;
        while frame_addr < end_frame {
            let _ = mem_obj.add_frame(frame_addr);
            frame_addr += 4096;
        }

        let mem_handle = system
            .insert_root(
                &mut cspace,
                &mut domain,
                mem_id,
                gaxera_abi::ObjectType::MemoryObject,
                gaxera_abi::Rights::READ | gaxera_abi::Rights::MAP | gaxera_abi::Rights::MANAGE,
                &arena,
            )
            .map_err(|_| InitError::StackAllocationFailed)?;
        if bootstrap_count == gaxera_abi::boot::MAX_BOOTSTRAP_CAPABILITIES {
            return Err(InitError::StackAllocationFailed);
        }
        bootstrap_entries[bootstrap_count] = gaxera_abi::boot::BootstrapCapability {
            role: gaxera_abi::boot::BootstrapRole::BootModule as u16,
            object_type: gaxera_abi::ObjectType::MemoryObject as u8,
            flags: 0,
            rights: (gaxera_abi::Rights::READ
                | gaxera_abi::Rights::MAP
                | gaxera_abi::Rights::MANAGE)
                .bits(),
            handle: mem_handle,
            metadata: module.size,
        };
        bootstrap_count += 1;
        crate::global::MEMORY_OBJECTS.lock().insert(mem_id, mem_obj);
    }

    let manifest = gaxera_abi::boot::BootstrapManifest {
        magic: gaxera_abi::boot::BootstrapManifest::MAGIC,
        abi_version: gaxera_abi::boot::BootstrapManifest::ABI_VERSION,
        header_size: gaxera_abi::boot::BootstrapManifest::HEADER_SIZE,
        entry_size: gaxera_abi::boot::BootstrapManifest::ENTRY_SIZE,
        total_size: u32::from(gaxera_abi::boot::BootstrapManifest::HEADER_SIZE)
            + u32::from(gaxera_abi::boot::BootstrapManifest::ENTRY_SIZE) * bootstrap_count as u32,
        entry_count: bootstrap_count as u16,
        reserved: 0,
        process_token: aspace_id.raw(),
        parent_token: 0,
        entries: bootstrap_entries,
    };
    if manifest.validate().is_err() {
        return Err(InitError::StackAllocationFailed);
    }
    // SAFETY: The destination lies in the already mapped private init stack
    // page, and the manifest is immutable after this write.
    unsafe {
        core::ptr::write(
            manifest_addr_hhdm as *mut gaxera_abi::boot::BootstrapManifest,
            manifest,
        );
    }

    // SAFETY: Single threaded boot.
    let arch_thread = crate::arch::x86_64::thread::ArchThread {
        stack: kernel_stack,
        context: crate::arch::x86_64::context::Context::empty(),
        cr3: Some(
            x86_64::structures::paging::PhysFrame::from_start_address(x86_64::PhysAddr::new(
                init_pml4,
            ))
            .unwrap(),
        ),
    };

    // The process will be entered directly via `enter_user_mode` at the end of this function.
    let mut thread = kernel_core::thread::Thread::new(thread_id, Some(aspace_id), arch_thread);
    thread.set_cspace(cspace_id);
    // Set as the current thread on this CPU
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        let cpu = crate::arch::x86_64::cpu::get_cpu_local_mut();
        cpu.kernel_stack_top = kernel_stack_top;
        let scheduler = &mut *cpu.scheduler.get();
        if let Some(s) = scheduler {
            s.set_current_thread(Some(thread.id()));
            thread.make_runnable().unwrap();
            thread.make_running().unwrap();
        } else {
            let mut s = kernel_core::scheduler::Scheduler::try_new(64).unwrap();
            s.set_current_thread(Some(thread.id()));
            thread.make_runnable().unwrap();
            thread.make_running().unwrap();
            *scheduler = Some(s);
        }
    }

    let mut init_process = kernel_core::process::Process::new(init_process_id, domain.id());
    init_process
        .bind_address_space(aspace_id)
        .map_err(|_| InitError::StackAllocationFailed)?;
    init_process
        .bind_capability_space(cspace_id)
        .map_err(|_| InitError::StackAllocationFailed)?;
    init_process
        .bind_main_thread(thread_id)
        .map_err(|_| InitError::StackAllocationFailed)?;
    init_process
        .bind_bootstrap_manifest(
            manifest_frame.start_address().as_u64(),
            user_manifest_ptr,
            manifest.total_size,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    init_process
        .bind_bootstrap_factory(factory_id)
        .map_err(|_| InitError::StackAllocationFailed)?;
    init_process
        .prepare()
        .map_err(|_| InitError::StackAllocationFailed)?;
    init_process
        .configure_main_thread()
        .map_err(|_| InitError::StackAllocationFailed)?;
    init_process
        .make_runnable()
        .map_err(|_| InitError::StackAllocationFailed)?;
    init_process
        .make_running()
        .map_err(|_| InitError::StackAllocationFailed)?;

    // Initialize Global State
    crate::global::init(arena, system);
    crate::global::RESOURCE_DOMAINS
        .lock()
        .insert(domain.id().object_id(), domain);

    for (mapping_id, mapping) in device_mappings {
        crate::global::MAPPINGS.lock().insert(mapping_id, mapping);
    }

    // Insert into Registries
    use kernel_core::registry::ObjectRegistry;
    crate::global::ADDRESS_SPACES
        .lock()
        .insert(aspace_id, aspace);
    crate::global::CAPABILITY_SPACES
        .lock()
        .insert(cspace_id, cspace);
    crate::global::PROCESSES
        .lock()
        .insert(init_process_id, init_process);
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        crate::arch::x86_64::thread::THREADS.insert(thread);
    }

    // The idle thread is a kernel-owned scheduler context. It is not bound to
    // a process and is never placed in a user capability space.
    let idle_stack =
        crate::arch::x86_64::stack::KernelStack::allocate(page_tables, physical_allocator)
            .map_err(|_| InitError::StackAllocationFailed)?;
    let mut idle_rsp = idle_stack.top().as_u64();
    // context_switch restores six callee-saved registers and returns through
    // the seeded instruction pointer.
    // SAFETY: `idle_stack` owns the mapped stack and `idle_rsp` stays within
    // its initialized top region while constructing the initial context.
    unsafe {
        idle_rsp -= 8;
        *(idle_rsp as *mut u64) =
            crate::arch::x86_64::thread::idle_thread_entry as *const () as usize as u64;
        for _ in 0..6 {
            idle_rsp -= 8;
            *(idle_rsp as *mut u64) = 0;
        }
    }
    let idle_arch = crate::arch::x86_64::thread::ArchThread {
        stack: idle_stack,
        context: crate::arch::x86_64::context::Context { rsp: idle_rsp },
        cr3: None,
    };
    let mut idle_thread = kernel_core::thread::Thread::new(idle_thread_id, None, idle_arch);
    idle_thread
        .make_runnable()
        .map_err(|_| InitError::StackAllocationFailed)?;
    crate::arch::x86_64::thread::set_idle_thread(idle_thread_id);
    // SAFETY: boot is still single-threaded and the idle object is unpublished
    // except through the scheduler's private thread table.
    unsafe {
        crate::arch::x86_64::thread::THREADS.insert(idle_thread);
    }

    // Unlock PHYSICAL_ALLOCATOR before entering userspace, otherwise syscalls will deadlock
    drop(phys_alloc_guard);

    // Enter userspace
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        x86_64::registers::control::Cr3::write(
            x86_64::structures::paging::PhysFrame::from_start_address(x86_64::PhysAddr::new(
                init_pml4,
            ))
            .unwrap(),
            x86_64::registers::control::Cr3Flags::empty(),
        );
        crate::arch::x86_64::user::enter_user_mode(
            entry_point,
            user_rsp,
            user_manifest_ptr,
            core::mem::size_of::<gaxera_abi::boot::BootstrapManifest>() as u64,
        );
    }
}
