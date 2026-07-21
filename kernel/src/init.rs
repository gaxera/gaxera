use crate::arch::x86_64::paging::KernelPageTables;
use crate::memory::boot::BootContext;
use gaxera_abi::ObjectTypeSet;
use kernel_core::elf::error::ElfError;
use kernel_core::elf::parser::ElfParser;
use kernel_core::object::{Factory, ObjectArena, ObjectError, ResourceDomain};
use x86_64::structures::paging::{FrameAllocator, PageSize, PageTableFlags, Size4KiB};

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

pub fn spawn_init(
    boot_context: &'static BootContext,
    page_tables: &mut KernelPageTables,
    mut arena: ObjectArena,
    mut system: kernel_core::capability::CapabilitySystem,
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
    };
    let mut domain = ResourceDomain::new_for_test(domain_id, limits); // 1 = Domain 1 for init

    // Create a Factory capability that can produce all ObjectTypes
    let factory = Factory::new_for_test(&domain, ObjectTypeSet::ALL);

    let aspace_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::AddressSpace)?;
    let cspace_id = arena.create(
        &mut domain,
        factory,
        gaxera_abi::ObjectType::CapabilitySpace,
    )?;
    let thread_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::Thread)?;
    let factory_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::Factory)?;
    crate::global::FACTORIES.lock().insert(factory_id, factory);

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
            let phys_frame = physical_allocator
                .allocate_frame()
                .ok_or(InitError::StackAllocationFailed)?;

            // SAFETY: Verified isolated address space.
            unsafe {
                KernelPageTables::map_user_page_in_pml4(
                    init_pml4,
                    current_vaddr,
                    phys_frame,
                    flags,
                    physical_allocator,
                )
                .map_err(|_| InitError::StackAllocationFailed)?;
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

    // Allocate User Stack
    let user_stack_frame = physical_allocator
        .allocate_frame()
        .ok_or(InitError::StackAllocationFailed)?;
    let user_stack_vaddr = 0x0000_7FFF_FFFF_F000u64; // Traditional high user address
    crate::println!(
        "GAXERA: USER_STACK phys={:#018x}",
        user_stack_frame.start_address().as_u64()
    );

    // Map User Stack into the isolated AddressSpace
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        KernelPageTables::map_user_page_in_pml4(
            init_pml4,
            user_stack_vaddr,
            user_stack_frame,
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::NO_EXECUTE,
            physical_allocator,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    }

    // Set up BootInfo at the top of the stack via HHDM
    let page_size = Size4KiB::SIZE;
    let hhdm_stack_top =
        crate::memory::mapping::HHDM_BASE + user_stack_frame.start_address().as_u64() + page_size;
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

    let user_boot_info_ptr = user_stack_vaddr + page_size - boot_info_size;
    let user_rsp = user_boot_info_ptr & !0xF; // 16-byte align stack

    let entry_point = parser.header().e_entry;
    crate::println!(
        "GAXERA: ENTER_USER_MODE entry={:#018x} rsp={:#018x} arg0={:#018x}",
        entry_point,
        user_rsp,
        user_boot_info_ptr
    );

    let aspace = kernel_core::address_space::AddressSpace::new(aspace_id, x86_aspace);
    let mut cspace = kernel_core::capability::CapabilitySpace::try_new(&domain, 1024)
        .map_err(|_| InitError::StackAllocationFailed)?;

    // Insert initial capabilities (Handles 0, 1, 2, 3)
    let rights = gaxera_abi::Rights::ALL;
    let _h0 = system
        .insert_root(
            &mut cspace,
            &mut domain,
            aspace_id,
            gaxera_abi::ObjectType::AddressSpace,
            rights,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    let _h1 = system
        .insert_root(
            &mut cspace,
            &mut domain,
            cspace_id,
            gaxera_abi::ObjectType::CapabilitySpace,
            rights,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    let _h2 = system
        .insert_root(
            &mut cspace,
            &mut domain,
            thread_id,
            gaxera_abi::ObjectType::Thread,
            rights,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;
    let _h3 = system
        .insert_root(
            &mut cspace,
            &mut domain,
            factory_id,
            gaxera_abi::ObjectType::Factory,
            rights,
            &arena,
        )
        .map_err(|_| InitError::StackAllocationFailed)?;

    // Insert boot modules as MemoryObjects (Handles 4+)
    for module in boot_context.boot_modules() {
        let mem_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::MemoryObject)?;
        let mut mem_obj = kernel_core::memory::MemoryObject::new(mem_id, module.size);

        let start_frame = module.physical_address & !0xFFF;
        let end_frame = (module.physical_address + module.size + 0xFFF) & !0xFFF;
        let mut frame_addr = start_frame;
        while frame_addr < end_frame {
            mem_obj.add_frame(frame_addr);
            frame_addr += 4096;
        }

        system
            .insert_root(
                &mut cspace,
                &mut domain,
                mem_id,
                gaxera_abi::ObjectType::MemoryObject,
                rights,
                &arena,
            )
            .map_err(|_| InitError::StackAllocationFailed)?;
        crate::global::MEMORY_OBJECTS.lock().insert(mem_id, mem_obj);
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
    let mut thread = kernel_core::thread::Thread::new(thread_id, Some(aspace_id), arch_thread);
    thread.set_cspace(cspace_id);
    // Set as the current thread on this CPU
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        let cpu = crate::arch::x86_64::cpu::get_cpu_local_mut();
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

    // Initialize Global State
    crate::global::init(arena, system);
    crate::global::RESOURCE_DOMAINS.lock().push(domain);

    // Insert into Registries
    use kernel_core::registry::ObjectRegistry;
    crate::global::ADDRESS_SPACES
        .lock()
        .insert(aspace_id, aspace);
    crate::global::CAPABILITY_SPACES
        .lock()
        .insert(cspace_id, cspace);
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        crate::arch::x86_64::thread::THREADS.insert(thread);
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
        crate::arch::x86_64::user::enter_user_mode(entry_point, user_rsp, user_boot_info_ptr);
    }
}
