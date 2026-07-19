use crate::arch::x86_64::loader::{self, LoaderError};
use crate::arch::x86_64::paging::KernelPageTables;
use crate::memory::boot::BootContext;
use crate::memory::physical::SegmentedBitmapFrameAllocator;
use gaxera_abi::ObjectTypeSet;
use kernel_core::elf::error::ElfError;
use kernel_core::elf::parser::ElfParser;
use kernel_core::object::{Factory, ObjectArena, ObjectError, ResourceDomain};
use x86_64::structures::paging::{FrameAllocator, PageSize, PageTableFlags, Size4KiB};

#[derive(Clone, Copy, Debug)]
pub enum InitError {
    ModuleNotFound,
    ElfParse(ElfError),
    Loader(LoaderError),
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

impl From<LoaderError> for InitError {
    fn from(err: LoaderError) -> Self {
        Self::Loader(err)
    }
}

pub fn spawn_init(
    boot_context: &'static BootContext,
    page_tables: &mut KernelPageTables,
    physical_allocator: &mut SegmentedBitmapFrameAllocator,
    arena: &mut ObjectArena,
) -> Result<!, InitError> {
    let init_module = boot_context
        .find_module("init")
        .ok_or(InitError::ModuleNotFound)?;

    crate::println!(
        "GAXERA: INIT_MODULE found at phys={:#018x} size={}",
        init_module.physical_address,
        init_module.size
    );

    let module_data = unsafe {
        core::slice::from_raw_parts(
            (init_module.physical_address + crate::memory::mapping::HHDM_BASE) as *const u8,
            init_module.size as usize,
        )
    };

    let parser = ElfParser::new(module_data)?;
    loader::map_elf_segments(page_tables, physical_allocator, &parser)?;

    // Bootstrap Capability Graph
    let domain_id = kernel_core::object::ResourceDomainId::new_for_test(1);
    let limits = kernel_core::resource::ResourceLimits {
        objects: 1024,
        capabilities: 1024,
    };
    let mut domain = ResourceDomain::new_for_test(domain_id, limits); // 1 = Domain 1 for init

    // Create a Factory capability that can produce all ObjectTypes
    let factory = Factory::new_for_test(&domain, ObjectTypeSet::ALL);

    let _aspace_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::AddressSpace)?;
    let _cspace_id = arena.create(
        &mut domain,
        factory,
        gaxera_abi::ObjectType::CapabilitySpace,
    )?;
    let _thread_id = arena.create(&mut domain, factory, gaxera_abi::ObjectType::Thread)?;

    // Allocate Kernel Stack for init syscalls
    let kernel_stack_frame = physical_allocator
        .allocate_frame()
        .ok_or(InitError::StackAllocationFailed)?;
    let kernel_stack_top = crate::memory::mapping::HHDM_BASE
        + kernel_stack_frame.start_address().as_u64()
        + Size4KiB::SIZE;
    crate::println!(
        "GAXERA: KERNEL_STACK phys={:#018x} top={:#018x}",
        kernel_stack_frame.start_address().as_u64(),
        kernel_stack_top
    );
    unsafe {
        crate::arch::x86_64::cpu::set_kernel_stack_top(kernel_stack_top);
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

    // Map User Stack
    unsafe {
        page_tables
            .map_user_page(
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

    // Jump to user mode (never returns)
    unsafe {
        crate::arch::x86_64::user::enter_user_mode(entry_point, user_rsp, user_boot_info_ptr);
    }
}
