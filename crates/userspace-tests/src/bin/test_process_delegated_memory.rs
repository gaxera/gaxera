#![no_std]
#![cfg_attr(not(test), no_main)]

use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{Handle, ObjectType, ProcessControlOp, Rights};
use libgaxera::allocator::UserspaceAllocator;
use libgaxera::process::{ProcessBuildError, ProcessBuilder};
use libgaxera::syscall;

#[repr(align(8))]
struct AlignedImage([u8; 512]);

// A tiny real child image. It resolves the self address space and delegated
// memory from the bootstrap manifest, maps the memory, writes a sentinel, and
// exits. The parent verifies the write through its own mapping.
fn child_image() -> AlignedImage {
    let mut image = AlignedImage([0; 512]);
    let bytes = &mut image.0;

    bytes[0..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2;
    bytes[5] = 1;
    bytes[6] = 1;
    bytes[16..18].copy_from_slice(&2u16.to_le_bytes());
    bytes[18..20].copy_from_slice(&0x3eu16.to_le_bytes());
    bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
    bytes[24..32].copy_from_slice(&0x20_0000u64.to_le_bytes());
    bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
    bytes[48..50].copy_from_slice(&64u16.to_le_bytes());
    bytes[52..54].copy_from_slice(&64u16.to_le_bytes());
    bytes[54..56].copy_from_slice(&56u16.to_le_bytes());
    bytes[56..58].copy_from_slice(&1u16.to_le_bytes());

    let ph = 64;
    bytes[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes());
    bytes[ph + 4..ph + 8].copy_from_slice(&5u32.to_le_bytes());
    bytes[ph + 8..ph + 16].copy_from_slice(&120u64.to_le_bytes());
    bytes[ph + 16..ph + 24].copy_from_slice(&0x20_0000u64.to_le_bytes());
    bytes[ph + 24..ph + 32].copy_from_slice(&0x20_0000u64.to_le_bytes());
    bytes[ph + 32..ph + 40].copy_from_slice(&120u64.to_le_bytes());
    bytes[ph + 40..ph + 48].copy_from_slice(&120u64.to_le_bytes());
    bytes[ph + 48..ph + 56].copy_from_slice(&0x1000u64.to_le_bytes());

    let mut code = [0u8; 320];
    let mut len = 0usize;
    macro_rules! emit {
        ($part:expr) => {{
            let part: &[u8] = $part;
            code[len..len + part.len()].copy_from_slice(part);
            len += part.len();
        }};
    }
    // Resolve the address-space and delegated memory handles from the
    // manifest.  The child must not depend on a creator-specific slot order.
    emit!(&[0x31, 0xc9]); // xor ecx,ecx: manifest entry index
    emit!(&[0x45, 0x31, 0xe4]); // xor r12d,r12d: address-space handle
    emit!(&[0x45, 0x31, 0xed]); // xor r13d,r13d: memory handle
    let scan_loop = len;
    emit!(&[0x66, 0x3b, 0x4f, 0x14]); // cmp cx,[rdi+entry_count]
    let scan_done = len;
    emit!(&[0x0f, 0x83, 0, 0, 0, 0]); // jae scan_done
    emit!(&[0x49, 0x89, 0xce]); // mov r14,rcx
    emit!(&[0x4f, 0x8d, 0x3c, 0x76]); // lea r15,[r14+r14*2]
    emit!(&[0x49, 0xc1, 0xe7, 0x03]); // shl r15,3: index * 24
    emit!(&[0x4e, 0x8d, 0x74, 0x3f, 0x28]); // lea r14,[rdi+r15+40]
    emit!(&[0x41, 0x0f, 0xb7, 0x06]); // movzx eax,word [r14]
    emit!(&[0x83, 0xf8, 0x00]); // cmp eax,SelfAddressSpace
    let check_memory = len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]); // jne check_memory
    emit!(&[0x4d, 0x8b, 0x66, 0x08]); // mov r12,[r14+8]
    let scan_next_from_aspace = len;
    emit!(&[0xe9, 0, 0, 0, 0]); // jmp scan_next
    let memory_label = len;
    emit!(&[0x83, 0xf8, 0x08]); // cmp eax,DeviceMemory
    let scan_next_from_other = len;
    emit!(&[0x0f, 0x85, 0, 0, 0, 0]); // jne scan_next
    emit!(&[0x4d, 0x8b, 0x6e, 0x08]); // mov r13,[r14+8]
    let scan_next = len;
    emit!(&[0x66, 0xff, 0xc1]); // inc cx
    let scan_back = len;
    emit!(&[0xe9, 0, 0, 0, 0]); // jmp scan_loop
    let scan_finished = len;
    emit!(&[0x4d, 0x85, 0xe4]); // test r12,r12
    let no_handles = len;
    emit!(&[0x0f, 0x84, 0, 0, 0, 0]); // je no_handles/fail
    emit!(&[0x4d, 0x85, 0xed]); // test r13,r13
    let no_memory = len;
    emit!(&[0x0f, 0x84, 0, 0, 0, 0]); // je no_memory/fail
    let continue_map = len;
    emit!(&[0xe9, 0, 0, 0, 0]); // skip diagnostic exit
    let no_handles_exit = len;
    emit!(&[0x31, 0xff, 0xbe, 99, 0, 0, 0, 0xba, 0xe2, 0, 0, 0]);
    let no_handles_exit_jump = len;
    emit!(&[0xeb, 0]); // patched to common exit syscall
    let map_setup = len;
    emit!(&[0x4c, 0x89, 0xe7]); // mov rdi,r12: address-space capability
    emit!(&[0x4c, 0x89, 0xea]); // mov rdx,r13: memory capability
    emit!(&[0x48, 0xc7, 0xc6, 1, 0, 0, 0]); // mov rsi,MapMemory
    emit!(&[0x49, 0xba, 0, 0, 0, 0, 0x30, 0, 0, 0]); // mov r10,0x30000000
    emit!(&[0x41, 0xb8, 3, 0, 0, 0]); // READ|WRITE
    emit!(&[0xb8, 10, 0, 0, 0, 0x0f, 0x05]); // sys_invoke
    emit!(&[0x48, 0x85, 0xc0]); // test rax,rax
    let jump = len;
    emit!(&[0x74, 0]); // je success
    emit!(&[0x31, 0xff, 0xbe, 99, 0, 0, 0, 0xba, 0xe1, 0, 0, 0]);
    let exit_jump = len;
    emit!(&[0xeb, 0]);
    let success = len;
    emit!(&[0x48, 0xb8]);
    emit!(&0x1122_3344_5566_7788u64.to_le_bytes());
    emit!(&[0x49, 0x89, 0x02]); // mov [r10],rax
    emit!(&[0x31, 0xff, 0xbe, 99, 0, 0, 0, 0xba, 0x42, 0, 0, 0]);
    let exit = len;
    emit!(&[0xb8, 10, 0, 0, 0, 0x0f, 0x05, 0xeb, 0xfe]);
    code[jump + 1] = (success as isize - (jump + 2) as isize) as i8 as u8;
    code[exit_jump + 1] = (exit as isize - (exit_jump + 2) as isize) as i8 as u8;
    let patch_rel32 = |code: &mut [u8], at: usize, target: usize| {
        let displacement = (target as isize - (at + 6) as isize) as i32;
        code[at + 2..at + 6].copy_from_slice(&displacement.to_le_bytes());
    };
    let patch_jump = |code: &mut [u8], at: usize, target: usize| {
        let displacement = (target as isize - (at + 5) as isize) as i32;
        code[at + 1..at + 5].copy_from_slice(&displacement.to_le_bytes());
    };
    patch_rel32(&mut code, scan_done, scan_finished);
    patch_rel32(&mut code, check_memory, memory_label);
    patch_jump(&mut code, scan_next_from_aspace, scan_next);
    patch_rel32(&mut code, scan_next_from_other, scan_next);
    let scan_back_disp = (scan_loop as isize - (scan_back + 5) as isize) as i32;
    code[scan_back + 1..scan_back + 5].copy_from_slice(&scan_back_disp.to_le_bytes());
    let no_handles_disp = (no_handles_exit as isize - (no_handles + 6) as isize) as i32;
    code[no_handles + 2..no_handles + 6].copy_from_slice(&no_handles_disp.to_le_bytes());
    let no_memory_disp = (no_handles_exit as isize - (no_memory + 6) as isize) as i32;
    code[no_memory + 2..no_memory + 6].copy_from_slice(&no_memory_disp.to_le_bytes());
    let continue_map_disp = (map_setup as isize - (continue_map + 5) as isize) as i32;
    code[continue_map + 1..continue_map + 5].copy_from_slice(&continue_map_disp.to_le_bytes());
    code[no_handles_exit_jump + 1] =
        (exit as isize - (no_handles_exit_jump + 2) as isize) as i8 as u8;
    bytes[120..120 + len].copy_from_slice(&code[..len]);
    bytes[40..48].copy_from_slice(&(64u64).to_le_bytes());
    bytes[32..40].copy_from_slice(&(64u64).to_le_bytes());
    bytes[88..96].copy_from_slice(&(len as u64).to_le_bytes());
    bytes[96..104].copy_from_slice(&(len as u64).to_le_bytes());
    bytes[104..112].copy_from_slice(&(len as u64).to_le_bytes());
    image
}

#[cfg(not(test))]
#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[unsafe(no_mangle)]
pub extern "C" fn _start(pointer: *const BootstrapManifest, length: usize) -> ! {
    // SAFETY: The kernel supplies a validated bootstrap manifest pointer and
    // length for this process entry.
    let manifest = match unsafe {
        libgaxera::entry::initialize_userspace_allocator(pointer, length, &ALLOCATOR)
    } {
        Ok(manifest) => manifest,
        Err(_) => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let factory = match capability(manifest, BootstrapRole::HeapFactory) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let aspace = match capability(manifest, BootstrapRole::SelfAddressSpace) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let image_factory = match capability(manifest, BootstrapRole::ImageFactory) {
        Some(handle) => handle,
        None => syscall::exit(gaxera_abi::status::INVALID_ARGUMENT),
    };
    let console = match syscall::factory_create(factory, ObjectType::DebugConsole) {
        Ok(console) => console,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_STEP_READY\n");
    let memory = match syscall::factory_create_memory_object(factory, 4096) {
        Ok(memory) => memory,
        Err(_) => syscall::exit(gaxera_abi::status::RESOURCE_EXHAUSTED),
    };
    let image = child_image();
    let first = match ProcessBuilder::new(factory, aspace, &image.0)
        .with_image_factory(image_factory)
        .install_capability(
            BootstrapRole::DeviceMemory as u16,
            memory,
            Rights::MAP | Rights::READ | Rights::WRITE,
        )
        .and_then(|builder| {
            builder.install_capability(
                BootstrapRole::ServiceEndpoint as u16,
                console,
                Rights::WRITE,
            )
        })
        .and_then(|builder| builder.spawn())
    {
        Ok(process) => process,
        Err(error) => {
            let message = match error {
                ProcessBuildError::ElfParseFailed => "GAXERA: DELEGATED_BUILD_ELF_FAIL\n",
                ProcessBuildError::CreateProcessFailed => "GAXERA: DELEGATED_BUILD_CREATE_FAIL\n",
                ProcessBuildError::AcquireComponentFailed => {
                    "GAXERA: DELEGATED_BUILD_COMPONENT_FAIL\n"
                }
                ProcessBuildError::MemoryAllocationFailed => {
                    "GAXERA: DELEGATED_BUILD_MEMORY_FAIL\n"
                }
                ProcessBuildError::TemporaryMappingFailed => {
                    "GAXERA: DELEGATED_BUILD_TEMP_MAP_FAIL\n"
                }
                ProcessBuildError::ChildMappingFailed => "GAXERA: DELEGATED_BUILD_CHILD_MAP_FAIL\n",
                ProcessBuildError::StackMappingFailed => "GAXERA: DELEGATED_BUILD_STACK_FAIL\n",
                ProcessBuildError::CapabilityInstallFailed => "GAXERA: DELEGATED_BUILD_CAP_FAIL\n",
                ProcessBuildError::ThreadConfigurationFailed => {
                    "GAXERA: DELEGATED_BUILD_THREAD_FAIL\n"
                }
                ProcessBuildError::StartFailed => "GAXERA: DELEGATED_BUILD_START_FAIL\n",
            };
            let _ = syscall::debug_console_write(console, message);
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR)
        }
    };
    let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_FIRST_STARTED\n");
    match wait_for_zombie(first) {
        Ok(0x42) => {}
        Ok(0xe1) => {
            let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_CHILD_MAP_FAILED\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
        }
        Ok(_) | Err(_) => syscall::exit(gaxera_abi::status::INTERNAL_ERROR),
    }
    let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_FIRST_EXITED\n");
    syscall::process_control(first, ProcessControlOp::Reap, 0, 0, 0).ok();

    let second = match ProcessBuilder::new(factory, aspace, &image.0)
        .with_image_factory(image_factory)
        .install_capability(
            BootstrapRole::DeviceMemory as u16,
            memory,
            Rights::MAP | Rights::READ | Rights::WRITE,
        )
        .and_then(|builder| {
            builder.install_capability(
                BootstrapRole::ServiceEndpoint as u16,
                console,
                Rights::WRITE,
            )
        })
        .and_then(|builder| builder.spawn())
    {
        Ok(process) => process,
        Err(_) => syscall::exit(gaxera_abi::status::INTERNAL_ERROR),
    };
    let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_STARTED\n");
    match wait_for_zombie(second) {
        Ok(0x42) => {}
        Ok(0xe1) => {
            let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_MAP_FAILED\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
        }
        Ok(0xe2) => {
            let _ =
                syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_MANIFEST_FAILED\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
        }
        Ok(0xDEAD_0000) => {
            let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_CHILD_PANIC\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
        }
        Ok(_) | Err(1) => {
            let _ =
                syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_YIELD_FAILED\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
        }
        Err(2) => {
            let _ =
                syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_WAIT_TIMEOUT\n");
            let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_EXIT_FAILED\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
        }
        Err(_) => {
            let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_WAIT_FAILED\n");
            syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
        }
    }
    let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_SECOND_EXITED\n");
    syscall::process_control(second, ProcessControlOp::Reap, 0, 0, 0).ok();

    let mapping = match syscall::map_memory(aspace, memory, 0x3000_0000, Rights::READ) {
        Ok(mapping) => mapping,
        Err(_) => syscall::exit(gaxera_abi::status::INTERNAL_ERROR),
    };
    // SAFETY: The supervisor mapped the same MemoryObject read-only after the
    // child processes exited; the child wrote this sentinel before exit.
    if unsafe { core::ptr::read_volatile(0x3000_0000 as *const u64) } != 0x1122_3344_5566_7788 {
        syscall::exit(gaxera_abi::status::INTERNAL_ERROR);
    }
    syscall::unmap_memory(mapping).ok();
    syscall::delete_handle(mapping).ok();
    syscall::delete_handle(memory).ok();
    let _ = syscall::debug_console_write(console, "GAXERA: DELEGATED_MEMORY_OK\n");
    syscall::exit(0);
}

#[cfg(not(test))]
fn capability(manifest: &BootstrapManifest, role: BootstrapRole) -> Option<Handle> {
    manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| entry.role == role as u16)
        .map(|entry| entry.handle)
}

#[cfg(not(test))]
fn wait_for_zombie(process: Handle) -> Result<u64, u8> {
    for _ in 0..10_000 {
        if let Ok((state, status)) = syscall::process_query(process)
            && state == 6
        {
            return Ok(status);
        }
        syscall::yield_now().map_err(|_| 1)?;
    }
    Err(2)
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    syscall::exit(0xDEAD_0000);
}
