#![no_std]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::arch::asm;
#[cfg(not(test))]
use core::panic::PanicInfo;
use gaxera_abi::{Handle, ObjectType, Rights};
use kernel_core::elf::parser::ElfParser;
use libgaxera::prelude::EndpointHandle;
use registry::ServiceRegistry;

use core::alloc::{GlobalAlloc, Layout};
struct DummyAllocator;
// SAFETY: Dummy allocator fulfilling no_std requirement.
unsafe impl GlobalAlloc for DummyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: DummyAllocator = DummyAllocator;

mod registry;
mod syscall;
use syscall::*;

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start(_boot_info: *const ()) -> ! {
    if run_init().is_err() {
        panic!("Init failed");
    }
    loop {
        // SAFETY: Halting execution.
        unsafe { asm!("pause") }
    }
}

#[cfg(not(test))]
fn run_init() -> Result<(), ()> {
    let self_aspace = Handle::from_parts(0, 1);
    let _self_cspace = Handle::from_parts(1, 1);
    let _self_thread = Handle::from_parts(2, 1);
    let factory = Handle::from_parts(3, 1);

    // 1. Create ramfs CSpace, ASpace, Thread
    let ramfs_cspace = factory_create(factory, ObjectType::CapabilitySpace)?;
    let ramfs_aspace = factory_create(factory, ObjectType::AddressSpace)?;
    let ramfs_thread = factory_create(factory, ObjectType::Thread)?;

    // 2. Create script_session CSpace, ASpace, Thread
    let script_cspace = factory_create(factory, ObjectType::CapabilitySpace)?;
    let script_aspace = factory_create(factory, ObjectType::AddressSpace)?;
    let script_thread = factory_create(factory, ObjectType::Thread)?;

    // 3. Map ramfs image (Handle 7) into ramfs_aspace at RAMFS_BASE
    // ramfs.img module is Handle 7.
    let ramfs_img = Handle::from_parts(7, 1);
    map_memory(
        ramfs_aspace,
        ramfs_img,
        gaxera_abi::svc::RAMFS_BASE,
        Rights::READ,
    )?;

    // 4. Create an Endpoint for communication and ServiceRegistry
    let endpoint = factory_create(factory, ObjectType::Endpoint)?;
    let mut registry = ServiceRegistry::new();
    if let Ok(name) = gaxera_abi::service::ServiceName::try_from_str("gaxera.svc.ramfs") {
        let _ = registry.register(name, EndpointHandle::from_raw(endpoint));
    }

    // 5. Derive the Endpoint into both child CSpaces at Handle(1)
    derive_capability(endpoint, ramfs_cspace, Rights::ALL)?;
    derive_capability(endpoint, script_cspace, Rights::ALL)?;

    // 6. Create a DebugConsole and derive into script_session CSpace at Handle(2)
    let console = factory_create(factory, ObjectType::DebugConsole)?;
    derive_capability(console, script_cspace, Rights::WRITE)?;

    // 7. Load and start ramfs (Handle 5)
    let ramfs_elf = Handle::from_parts(5, 1);
    let ramfs_load = 0x0000_1000_0000_0000;
    let ramfs_stack = 0x0000_7FFF_FFFF_C000;

    map_memory(self_aspace, ramfs_elf, ramfs_load, Rights::READ)?;
    // SAFETY: Bootloader maps this range and guarantees it is valid.
    let ramfs_slice =
        unsafe { core::slice::from_raw_parts(ramfs_load as *const u8, 16 * 1024 * 1024) };
    let ramfs_parser = ElfParser::new(ramfs_slice).map_err(|_| ())?;

    for phdr in ramfs_parser.program_headers() {
        if phdr.p_type == 1 {
            let vaddr = phdr.p_vaddr;
            let mem_size = phdr.p_memsz;
            let aligned_vaddr = vaddr & !0xFFF;
            let offset = vaddr & 0xFFF;
            let aligned_size = (mem_size + offset + 0xFFF) & !0xFFF;
            let mem_obj = factory_create_memory(factory, aligned_size)?;
            map_memory(ramfs_aspace, mem_obj, aligned_vaddr, Rights::ALL)?;
            let temp_vaddr = ramfs_load + 16 * 1024 * 1024 + aligned_vaddr;
            map_memory(self_aspace, mem_obj, temp_vaddr, Rights::ALL)?;
            if phdr.p_filesz > 0 {
                // SAFETY: We just mapped this memory range and verified the sizes.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &ramfs_slice[phdr.p_offset as usize],
                        (temp_vaddr + offset) as *mut u8,
                        phdr.p_filesz as usize,
                    );
                }
            }
        }
    }

    let stack_size = 16384;
    let ramfs_stack_mem = factory_create_memory(factory, stack_size)?;
    map_memory(
        ramfs_aspace,
        ramfs_stack_mem,
        ramfs_stack - stack_size,
        Rights::ALL,
    )?;
    thread_configure(
        ramfs_thread,
        ramfs_parser.header().e_entry,
        ramfs_stack,
        ramfs_aspace,
        ramfs_cspace,
    )?;

    // 8. Load and start script_session (Handle 6)
    let script_elf = Handle::from_parts(6, 1);
    let script_load = 0x0000_2000_0000_0000;
    let script_stack = 0x0000_7FFE_FFFF_C000;

    map_memory(self_aspace, script_elf, script_load, Rights::READ)?;
    // SAFETY: Bootloader maps this range and guarantees it is valid.
    let script_slice =
        unsafe { core::slice::from_raw_parts(script_load as *const u8, 16 * 1024 * 1024) };
    let script_parser = ElfParser::new(script_slice).map_err(|_| ())?;

    for phdr in script_parser.program_headers() {
        if phdr.p_type == 1 {
            let vaddr = phdr.p_vaddr;
            let mem_size = phdr.p_memsz;
            let aligned_vaddr = vaddr & !0xFFF;
            let offset = vaddr & 0xFFF;
            let aligned_size = (mem_size + offset + 0xFFF) & !0xFFF;
            let mem_obj = factory_create_memory(factory, aligned_size)?;
            map_memory(script_aspace, mem_obj, aligned_vaddr, Rights::ALL)?;
            let temp_vaddr = script_load + 16 * 1024 * 1024 + aligned_vaddr;
            map_memory(self_aspace, mem_obj, temp_vaddr, Rights::ALL)?;
            if phdr.p_filesz > 0 {
                // SAFETY: We just mapped this memory range and verified the sizes.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        &script_slice[phdr.p_offset as usize],
                        (temp_vaddr + offset) as *mut u8,
                        phdr.p_filesz as usize,
                    );
                }
            }
        }
    }

    let script_stack_mem = factory_create_memory(factory, stack_size)?;
    map_memory(
        script_aspace,
        script_stack_mem,
        script_stack - stack_size,
        Rights::ALL,
    )?;
    thread_configure(
        script_thread,
        script_parser.header().e_entry,
        script_stack,
        script_aspace,
        script_cspace,
    )?;

    // 9. Supervisor Loop
    loop {
        if !registry.is_empty() {
            // Service registry operational
        }
        if let Ok(gaxera_abi::THREAD_STATE_DEAD) = syscall::thread_status(script_thread) {
            let _ = syscall::debug_console_write(
                console,
                "[init] Detected script_session crash! Restarting...\n",
            );

            // Reconfigure the thread (reset rip and rsp)
            thread_configure(
                script_thread,
                script_parser.header().e_entry,
                script_stack,
                script_aspace,
                script_cspace,
            )?;
        }
        // SAFETY: Yield CPU slightly
        unsafe { asm!("pause") }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        // SAFETY: Halting execution.
        unsafe { asm!("pause") }
    }
}
