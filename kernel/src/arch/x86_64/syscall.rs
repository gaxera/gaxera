#![allow(
    clippy::undocumented_unsafe_blocks,
    clippy::collapsible_if,
    clippy::let_and_return
)]

use crate::arch::x86_64::cpu;
use core::arch::global_asm;
use x86_64::registers::model_specific::{Efer, EferFlags, Msr};
use x86_64::registers::rflags::RFlags;

use crate::memory::mapping::USER_ADDRESS_MAX;
use crate::println;
use kernel_core::address_space::ArchAddressSpace;
use kernel_core::registry::ObjectRegistry;
use x86_64::structures::paging::{FrameAllocator, FrameDeallocator};

const MSR_STAR: u32 = 0xC0000081;
const MSR_LSTAR: u32 = 0xC0000082;
const MSR_FMASK: u32 = 0xC0000084;

/// An RAII guard ensuring that a MemoryObject transient pin is exactly-once decremented
/// when dropped, avoiding leaks across complex MapMemory failure and success paths.
pub struct MemoryObjectTransientPin {
    object_id: kernel_core::object::ObjectId,
}

impl Drop for MemoryObjectTransientPin {
    fn drop(&mut self) {
        let can_destroy = {
            let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
            mem_objects
                .get_mut(self.object_id)
                .and_then(|mem_obj| mem_obj.dec_transient_ref().ok())
                .unwrap_or(false)
        };

        if can_destroy {
            reclaim_memory_object_if_zero_refs(self.object_id);
        }
    }
}

#[allow(
    clippy::collapsible_if,
    clippy::unnecessary_cast,
    clippy::undocumented_unsafe_blocks,
    unused_unsafe
)]
pub(crate) fn reclaim_memory_object_if_zero_refs(object_id: kernel_core::object::ObjectId) {
    let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
    let mem_obj = match mem_objects.get(object_id) {
        Some(m) if m.can_destroy() => match mem_objects.remove(object_id) {
            Some(removed) => removed,
            None => return,
        },
        _ => return,
    };
    drop(mem_objects);

    let domain_id = mem_obj.charging_domain();
    let size_bytes = mem_obj.size_bytes();

    // Deallocate physical frames directly over &[u64] slice (allocation-free)
    {
        let mut phys_alloc = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = phys_alloc.as_mut() {
            use x86_64::structures::paging::FrameDeallocator;
            for &f in mem_obj.frames() {
                // SAFETY: Frame returned to physical allocator upon object destruction.
                unsafe {
                    allocator.deallocate_frame(
                        x86_64::structures::paging::PhysFrame::containing_address(
                            x86_64::PhysAddr::new(f),
                        ),
                    );
                }
            }
        }
    }

    // Refund ResourceDomain byte quota
    {
        let mut domains = crate::global::RESOURCE_DOMAINS.lock();
        if let Some(domain) = domains.get_mut(domain_id.object_id()) {
            let _ = domain.release_memory(size_bytes);
            let _ = domain.release_object();
        }
    }

    // Destroy object from ObjectArena
    {
        let mut domains = crate::global::RESOURCE_DOMAINS.lock();
        let mut arena_guard = crate::global::OBJECT_ARENA.lock();
        if let (Some(arena), Some(domain)) =
            (arena_guard.as_mut(), domains.get_mut(domain_id.object_id()))
        {
            let _ = arena.destroy(domain, object_id);
        }
    }
}

pub(crate) fn reclaim_contiguous_frame_if_zero_refs(object_id: kernel_core::object::ObjectId) {
    let mut frames = crate::global::CONTIGUOUS_FRAMES.lock();
    let frame_obj = match frames.get(object_id) {
        Some(frame_obj) if frame_obj.can_destroy() => match frames.remove(object_id) {
            Some(frame_obj) => frame_obj,
            None => return,
        },
        _ => return,
    };
    drop(frames);

    let owner = frame_obj.owner();
    let page_count = frame_obj.page_count();
    let base_frame = frame_obj.base_frame();

    {
        let mut allocator = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = allocator.as_deref_mut() {
            let _ = allocator.deallocate_contiguous(base_frame, page_count);
        }
    }

    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
    if let Some(domain) = domains.get_mut(owner.object_id()) {
        let bytes = (page_count as u64).saturating_mul(crate::memory::physical::PAGE_SIZE);
        let _ = domain.release_memory(bytes);
        let _ = domain.release_object();
    }
    drop(domains);

    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
    let mut arena = crate::global::OBJECT_ARENA.lock();
    if let Some(arena) = arena.as_mut()
        && let Some(domain) = domains.get_mut(owner.object_id())
    {
        let _ = arena.destroy(domain, object_id);
    }
}

#[allow(
    clippy::collapsible_if,
    clippy::unnecessary_cast,
    clippy::undocumented_unsafe_blocks,
    unused_unsafe
)]
pub fn perform_kernel_selective_revoke(revoked_node: kernel_core::capability::CapabilityNodeId) {
    // 1. Revoke descendant capabilities iteratively
    loop {
        let mut system = crate::global::CAPABILITY_SYSTEM.lock();
        let revoked_info = if let Some(sys) = system.as_mut() {
            sys.revoke_one_descendant(revoked_node)
        } else {
            None
        };
        drop(system);

        match revoked_info {
            Some(_) => {} // Node marked revoked in CapabilitySystem; slot deletion happens via DeleteHandle
            None => break, // No more active descendants found
        }
    }

    // 2. Tear down descendant mappings iteratively
    loop {
        let system_guard = crate::global::CAPABILITY_SYSTEM.lock();
        let mut mappings_guard = crate::global::MAPPINGS.lock();
        let mut found = None;

        if let Some(sys) = system_guard.as_ref() {
            for (id, m) in mappings_guard.iter_mut() {
                if let Some(parent_node) = m.lineage_parent_node() {
                    if sys.is_descendant_of(parent_node, revoked_node) {
                        if !m.is_closed() {
                            let _ = m.close();
                            found = Some((
                                id,
                                m.target_address_space(),
                                m.virtual_address(),
                                m.size() as u64,
                                m.backing().clone(),
                            ));
                            break;
                        }
                    }
                }
            }
        }
        drop(system_guard);
        drop(mappings_guard);

        if let Some((m_id, aspace_id, vaddr, size, backing)) = found {
            // Unmap page tables
            let mut aspaces = crate::global::ADDRESS_SPACES.lock();
            if let Some(aspace) = aspaces.get_mut(aspace_id) {
                use kernel_core::address_space::ArchAddressSpace;
                let _ = aspace.arch.unmap_range(vaddr, (size / 4096) as usize);
            }
            drop(aspaces);

            // Multi-page TLB flush
            let mut flush_vaddr = vaddr;
            let end_vaddr = vaddr + size;
            while flush_vaddr < end_vaddr {
                // SAFETY: TLB flush for revoked virtual address range.
                unsafe {
                    x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(flush_vaddr));
                }
                flush_vaddr += 4096;
            }

            // Remove mapping from registry
            let mut mappings = crate::global::MAPPINGS.lock();
            let _ = mappings.remove(m_id);
            drop(mappings);

            // Destroy Mapping in ObjectArena
            let mut domains = crate::global::RESOURCE_DOMAINS.lock();
            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
            if let Some(arena) = arena_guard.as_mut() {
                if let Some(owner_id) = arena.owner(m_id) {
                    if let Some(domain) = domains.get_mut(owner_id.object_id()) {
                        let _ = arena.destroy(domain, m_id);
                    }
                }
            }
            drop(arena_guard);
            drop(domains);

            // Decrement MemoryObject mapping reference
            if let kernel_core::mapping::MappingBacking::MemoryObject { object_id, .. } = backing {
                let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
                let can_destroy = if let Some(mem_obj) = mem_objects.get_mut(object_id) {
                    mem_obj.dec_mapping_ref().unwrap_or(false)
                } else {
                    false
                };
                drop(mem_objects);

                if can_destroy {
                    reclaim_memory_object_if_zero_refs(object_id);
                }
            }
        } else {
            break; // No more matching mappings
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SyscallFrame {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64, // RFLAGS
    pub r10: u64, // Arg 3 (rcx holds rip, so r10 is arg 3)
    pub r9: u64,  // Arg 5
    pub r8: u64,  // Arg 4
    pub rbp: u64,
    pub rdi: u64, // Arg 0
    pub rsi: u64, // Arg 1
    pub rdx: u64, // Arg 2
    pub rcx: u64, // RIP
    pub rax: u64, // Syscall number
    pub rsp: u64, // User RSP
}

global_asm!(
    r#"
    .global syscall_entry
    .extern handle_syscall
    syscall_entry:
        // 1. Swap GS base to access CpuLocal
        swapgs

        // 2. Save user RSP to scratch space in CpuLocal (offset 8)
        mov gs:[8], rsp

        // 3. Load kernel_stack_top from CpuLocal (offset 0) into RSP
        mov rsp, gs:[0]

        // 4. Construct SyscallFrame on kernel stack
        push gs:[8]      // User RSP
        push rax         // Syscall number
        push rcx         // User RIP
        push rdx         // Arg 2
        push rsi         // Arg 1
        push rdi         // Arg 0
        push rbp
        push r8          // Arg 4
        push r9          // Arg 5
        push r10         // Arg 3
        push r11         // User RFLAGS
        push r12
        push r13
        push r14
        push r15

        // Pass pointer to frame as first arg (&mut SyscallFrame)
        // Align stack to 16-bytes before call (ABI requirement)
        // Currently 15 pushes * 8 bytes = 120 bytes, so rsp is 16n + 8.
        // We push a dummy value (or sub rsp, 8) to make it 16n.
        mov rdi, rsp
        sub rsp, 8
        call handle_syscall
        add rsp, 8

    .global syscall_return
    syscall_return:
        // Restore registers
        pop r15
        pop r14
        pop r13
        pop r12
        pop r11          // Restore RFLAGS into R11 for sysret
        pop r10
        pop r9
        pop r8
        pop rbp
        pop rdi
        pop rsi
        pop rdx
        pop rcx          // Restore RIP into RCX for sysret
        pop rax          // Restore rax (return value)
        pop rsp          // Restore user RSP

        // Swap GS back to user GS
        swapgs
        sysretq
    "#
);

unsafe extern "C" {
    fn syscall_entry();
    fn syscall_return();
}

/// Validates the sysret return frame for safety.
///
/// `sysretq` loads RIP from RCX and RFLAGS from R11. If RCX contains a
/// non-canonical address, the processor raises `#GP(0)` while still at CPL 0
/// (the well-known sysret vulnerability). This function ensures the return
/// frame cannot trigger that condition or restore forbidden RFLAGS bits.
fn validate_sysret_frame(frame: &SyscallFrame) -> bool {
    // RCX (return RIP) and RSP must be non-zero lower-half canonical user
    // addresses. `sysretq` consumes RCX while still at CPL 0; RSP is restored
    // before the privilege transition in the entry assembly, so both fields
    // are part of the kernel return boundary.
    if !is_user_return_address(frame.rcx) || !is_user_return_address(frame.rsp) {
        return false;
    }

    // R11 (return RFLAGS):
    // - Bit 1 (fixed-one) must be set
    // - IF (bit 9) should be set for user mode
    // - IOPL (bits 12:13) must be zero
    // - NT (bit 14) must be clear
    // - VM (bit 17) must be clear
    // - AC (bit 18) must be clear
    let r11 = frame.r11;
    let rflags_fixed_one: u64 = 1 << 1;
    let rflags_forbidden: u64 = (3 << 12) | (1 << 14) | (1 << 17) | (1 << 18);

    if r11 & rflags_fixed_one == 0 {
        return false;
    }
    if r11 & rflags_forbidden != 0 {
        return false;
    }

    true
}

const fn is_user_return_address(address: u64) -> bool {
    address != 0 && address <= USER_ADDRESS_MAX
}

/// Enables x86_64 `syscall`/`sysret` hardware support.
///
/// # Safety
/// Must be called once during early BSP setup.
pub unsafe fn enable_syscalls() {
    // SAFETY: Hardware invariant or verified by caller.
    unsafe {
        // 1. Enable SCE (System Call Extensions) in EFER
        let current_efer = Efer::read();
        Efer::write(current_efer | EferFlags::SYSTEM_CALL_EXTENSIONS);

        // 2. Program STAR MSR
        // STAR[47:32] = Kernel CS (0x08). SYSRET loads CS = STAR[63:48] + 16 (0x10 + 16 = 0x20 | 3 = 0x23), SS = STAR[63:48] + 8 (0x10 + 8 = 0x18 | 3 = 0x1b)
        let star_val = (0x10_u64 << 48) | (0x08_u64 << 32);
        Msr::new(MSR_STAR).write(star_val);

        // 3. Program LSTAR MSR (syscall entry address)
        let entry_addr = syscall_entry as *const () as usize as u64;
        Msr::new(MSR_LSTAR).write(entry_addr);

        // 4. Program FMASK MSR (mask RFLAGS bits during syscall)
        // Mask IF (Interrupt Flag), TF (Trap Flag), DF (Direction Flag), etc.
        let mask = RFlags::INTERRUPT_FLAG.bits()
            | RFlags::TRAP_FLAG.bits()
            | RFlags::DIRECTION_FLAG.bits();
        Msr::new(MSR_FMASK).write(mask);
    }
}

pub fn sys_invoke(frame: &mut SyscallFrame) -> u64 {
    handle_syscall(frame);
    frame.rax
}

#[unsafe(no_mangle)]
#[allow(
    clippy::collapsible_if,
    clippy::unnecessary_cast,
    clippy::undocumented_unsafe_blocks,
    unused_unsafe
)]
pub extern "C" fn handle_syscall(frame: &mut SyscallFrame) {
    if frame.rax != 0 && frame.rax != 1 {
        // crate::println!(
        //     "GAXERA: SYSCALL rax={} handle={} op={}",
        //     frame.rax,
        //     frame.rdi,
        //     frame.rsi
        // );
    }
    // For M2B, handle simple syscalls like NoOp and Yield, or return error for unknown
    frame.rax = match frame.rax {
        0 => {
            // NoOp / Test Syscall
            0
        }
        1 => match yield_current_thread() {
            Ok(()) => 0,
            Err(()) => u64::MAX,
        },
        2 => {
            #[cfg(feature = "test-preemption")]
            {
                crate::println!("GAXERA: PREEMPTION_OK");
                // SAFETY: Hardware invariant or verified by caller.
                unsafe { crate::arch::x86_64::qemu::exit_success() };
            }
            #[cfg(not(feature = "test-preemption"))]
            u64::MAX
        }
        10 => 'sys_invoke: {
            // sys_invoke(handle_raw, op, ...)
            let handle_raw = frame.rdi;
            let handle = gaxera_abi::Handle::from_raw(handle_raw);

            // 1. Identify active Thread
            // SAFETY: Hardware invariant or verified by caller.
            let cpu_local = unsafe { cpu::get_cpu_local() };
            // SAFETY: Single CPU per thread invariant.
            let scheduler = unsafe { &*cpu_local.scheduler.get() };

            let current_thread_id = match scheduler.as_ref().and_then(|s| s.current_thread()) {
                Some(id) => id,
                None => {
                    crate::println!("GAXERA: current_thread() None");
                    break 'sys_invoke u64::MAX;
                }
            };
            let current_process_id = process_for_thread(current_thread_id);
            if let Some(pid) = current_process_id {
                let processes = crate::global::PROCESSES.lock();
                if let Some(process) = processes.get(pid) {
                    // The process state is published as Runnable before its
                    // first scheduler dispatch.  The current-thread lookup
                    // above is the authority that proves this thread is
                    // actually executing, so both Runnable and Running are
                    // valid syscall states.  Terminal states remain denied.
                    if !matches!(
                        process.state(),
                        kernel_core::process::ProcessState::Runnable
                            | kernel_core::process::ProcessState::Running
                    ) && frame.rsi != gaxera_abi::OperationCode::ExitProcess as u64
                    {
                        break 'sys_invoke gaxera_abi::status::INVALID_ARGUMENT;
                    }
                }
            }

            // 2. Identify CSpace
            // SAFETY: Hardware invariant or verified by caller.
            let cspace_id =
                match unsafe { crate::arch::x86_64::thread::THREADS.get(current_thread_id) } {
                    Some(t) => match t.cspace() {
                        Some(c) => c,
                        None => {
                            crate::println!("GAXERA: t.cspace() was None");
                            break 'sys_invoke u64::MAX;
                        }
                    },
                    None => {
                        crate::println!("GAXERA: THREADS.get failed");
                        break 'sys_invoke u64::MAX;
                    }
                };

            // 3. Capability Resolution
            // Limit the lock scope so we don't hold CAPABILITY_SYSTEM while invoking.
            if frame.rsi == gaxera_abi::OperationCode::CreateProcess as u64 {
                break 'sys_invoke create_process_syscall(handle, cspace_id, frame);
            }
            if frame.rsi == gaxera_abi::OperationCode::ProcessControl as u64 {
                break 'sys_invoke process_control_syscall(handle, cspace_id, frame);
            }
            if frame.rsi == gaxera_abi::OperationCode::FactoryCreate as u64 {
                break 'sys_invoke factory_create_syscall(handle, cspace_id, frame);
            }
            if frame.rsi == gaxera_abi::OperationCode::ExitProcess as u64 {
                let exit_code = frame.rdx;
                if let Some(process_id) = current_process_id {
                    exit_current_process(process_id, current_thread_id, exit_code);
                } else {
                    unsafe {
                        if let Some(thread) =
                            crate::arch::x86_64::thread::THREADS.get_mut(current_thread_id)
                        {
                            let _ = thread.make_dying();
                        }
                    }
                    let cpu_local = unsafe { cpu::get_cpu_local() };
                    let scheduler = unsafe { &mut *cpu_local.scheduler.get() };
                    if let Some(scheduler) = scheduler.as_mut() {
                        let _ = scheduler.remove_thread(current_thread_id);
                        if let Some(next_id) = scheduler.dequeue_next() {
                            scheduler.set_current_thread(Some(next_id));
                            let _ = crate::arch::x86_64::preemption::switch_to_next(
                                current_thread_id,
                                next_id,
                            );
                        }
                    }
                    crate::serial::halt();
                }
            }
            if frame.rsi == gaxera_abi::OperationCode::YieldProcess as u64 {
                break 'sys_invoke match yield_current_thread() {
                    Ok(()) => gaxera_abi::status::SUCCESS,
                    Err(()) => gaxera_abi::status::INTERNAL_ERROR,
                };
            }
            let sys_result = {
                let mut cspaces = crate::global::CAPABILITY_SPACES.lock();
                let cspaces_ptr = &mut *cspaces
                    as *mut kernel_core::registry::BTreeRegistry<
                        kernel_core::capability::CapabilitySpace,
                    >;
                let cspace = match cspaces.get_mut(cspace_id) {
                    Some(c) => c,
                    None => {
                        crate::println!("GAXERA: cspaces.get_mut failed for {:?}", cspace_id);
                        break 'sys_invoke u64::MAX;
                    }
                };

                let mut system = crate::global::CAPABILITY_SYSTEM.lock();
                let sys = match system.as_mut() {
                    Some(s) => s,
                    None => break 'sys_invoke u64::MAX,
                };

                let arena = crate::global::OBJECT_ARENA.lock();
                let arena_ref = match arena.as_ref() {
                    Some(a) => a,
                    None => break 'sys_invoke u64::MAX,
                };

                let op = frame.rsi;

                if op == gaxera_abi::OperationCode::MapMemory as u64 {
                    // map_memory(aspace_handle, mem_handle, vaddr, rights)
                    let aspace_result = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::AddressSpace,
                        gaxera_abi::Rights::MAP,
                        arena_ref,
                    );

                    let mem_handle = gaxera_abi::Handle::from_raw(frame.rdx);
                    let mem_info_result = sys.lookup_info(
                        cspace,
                        mem_handle,
                        gaxera_abi::ObjectType::MemoryObject,
                        gaxera_abi::Rights::MAP,
                        arena_ref,
                    );
                    let mem_result = mem_info_result.map(|info| info.object);
                    let mapping_result = if mem_result.is_err() {
                        sys.lookup(
                            cspace,
                            mem_handle,
                            gaxera_abi::ObjectType::Mapping,
                            gaxera_abi::Rights::MAP,
                            arena_ref,
                        )
                    } else {
                        Err(kernel_core::capability::CapabilityError::StaleHandle)
                    };
                    let mapping_rights = sys
                        .inspect(cspace, mem_handle, arena_ref)
                        .map(|info| info.rights);
                    let frame_result = if mem_result.is_err() && mapping_result.is_err() {
                        sys.lookup(
                            cspace,
                            mem_handle,
                            gaxera_abi::ObjectType::ContiguousFrame,
                            gaxera_abi::Rights::MAP | gaxera_abi::Rights::READ,
                            arena_ref,
                        )
                    } else {
                        Err(kernel_core::capability::CapabilityError::StaleHandle)
                    };

                    if let Ok(aspace_id) = aspace_result {
                        let virtual_address = frame.r10; // Arg 3
                        let requested_rights = gaxera_abi::Rights::from_bits(frame.r8 as u32); // Arg 4
                        let offset_bytes = frame.r9; // Arg 5
                        let length_bytes = 0_u64; // Default to full size

                        // 1. Enforce 4 KiB alignment and non-zero virtual address
                        if virtual_address == 0
                            || (virtual_address & 0xFFF) != 0
                            || (offset_bytes & 0xFFF) != 0
                        {
                            break 'sys_invoke u64::MAX;
                        }

                        if let Ok(mem_info) = mem_info_result {
                            let mem_id = mem_info.object;
                            // Rights-subset validation: requested rights must be a subset of source capability rights
                            if !requested_rights.is_subset_of(mem_info.rights) {
                                break 'sys_invoke u64::MAX;
                            }

                            // Reject simultaneous W+X permission for all memory objects
                            if requested_rights.contains(gaxera_abi::Rights::WRITE)
                                && requested_rights.contains(gaxera_abi::Rights::EXECUTE)
                            {
                                break 'sys_invoke u64::MAX;
                            }

                            let node_id = match sys.node_for(cspace, mem_handle) {
                                Ok(n) => n,
                                Err(_) => break 'sys_invoke u64::MAX,
                            };

                            let caller_domain_id = cspace.domain();

                            drop(arena);
                            drop(system);
                            drop(cspaces);

                            let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
                            let mem_obj = match mem_objects.get_mut(mem_id) {
                                Some(m) => m,
                                None => break 'sys_invoke u64::MAX,
                            };

                            // Reject EXECUTE rights for Anonymous memory objects
                            if mem_obj.kind() == kernel_core::memory::MemoryObjectKind::Anonymous
                                && requested_rights.contains(gaxera_abi::Rights::EXECUTE)
                            {
                                break 'sys_invoke u64::MAX;
                            }

                            let mem_size = mem_obj.size_bytes();
                            let map_length = if length_bytes == 0 {
                                mem_size.saturating_sub(offset_bytes)
                            } else {
                                length_bytes
                            };

                            if map_length == 0 || (map_length & 0xFFF) != 0 {
                                break 'sys_invoke u64::MAX;
                            }

                            let end_offset = match offset_bytes.checked_add(map_length) {
                                Some(o) if o <= mem_size => o,
                                _ => {
                                    break 'sys_invoke u64::MAX;
                                }
                            };
                            let _ = end_offset;

                            let is_valid_user_range = virtual_address
                                .checked_add(map_length)
                                .is_some_and(|end_vaddr| end_vaddr <= USER_ADDRESS_MAX);

                            if !is_valid_user_range {
                                break 'sys_invoke u64::MAX;
                            }

                            if mem_obj.inc_transient_ref().is_err() {
                                break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                            }
                            let _pin = MemoryObjectTransientPin { object_id: mem_id };

                            let mut frame_vec = alloc::vec::Vec::new();
                            match mem_obj.frames_subrange(offset_bytes, map_length) {
                                Ok(f) => {
                                    if frame_vec.try_reserve_exact(f.len()).is_err() {
                                        break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                    }
                                    frame_vec.extend_from_slice(f);
                                }
                                Err(_) => {
                                    break 'sys_invoke u64::MAX;
                                }
                            }
                            drop(mem_objects);

                            // Transaction step 1: Reserve object & charge caller ResourceDomain
                            let mut domain_guard = crate::global::RESOURCE_DOMAINS.lock();
                            let domain = match domain_guard.get_mut(caller_domain_id.object_id()) {
                                Some(d) => d,
                                None => break 'sys_invoke u64::MAX,
                            };

                            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                            let arena = match arena_guard.as_mut() {
                                Some(a) => a,
                                None => break 'sys_invoke u64::MAX,
                            };

                            let mapping_id = match arena.create_mapping(domain) {
                                Ok(id) => id,
                                Err(_) => {
                                    break 'sys_invoke u64::MAX;
                                }
                            };
                            drop(arena_guard);
                            drop(domain_guard);

                            // Transaction step 2: Map page tables
                            let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                            let aspace = match aspaces.get_mut(aspace_id) {
                                Some(a) => a,
                                None => {
                                    // Rollback arena
                                    drop(aspaces);
                                    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                                    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                                    if let Some(arena) = arena_guard.as_mut() {
                                        if let Some(domain) =
                                            domains.get_mut(caller_domain_id.object_id())
                                        {
                                            let _ = arena.destroy(domain, mapping_id);
                                        }
                                    }
                                    break 'sys_invoke u64::MAX;
                                }
                            };
                            use kernel_core::address_space::ArchAddressSpace;
                            if aspace
                                .arch
                                .map_frames(virtual_address, &frame_vec, requested_rights)
                                .is_err()
                            {
                                drop(aspaces);
                                let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                                let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                                if let Some(arena) = arena_guard.as_mut() {
                                    if let Some(domain) =
                                        domains.get_mut(caller_domain_id.object_id())
                                    {
                                        let _ = arena.destroy(domain, mapping_id);
                                    }
                                }
                                break 'sys_invoke u64::MAX;
                            }
                            drop(aspaces);

                            // Transaction step 3: Increment mapping ref
                            let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
                            if let Some(mem_obj) = mem_objects.get_mut(mem_id) {
                                if mem_obj.inc_mapping_ref().is_err() {
                                    drop(mem_objects);
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace.arch.unmap_range(
                                            virtual_address,
                                            (map_length / 4096) as usize,
                                        );
                                    }
                                    drop(aspaces);
                                    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                                    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                                    if let Some(arena) = arena_guard.as_mut() {
                                        if let Some(domain) =
                                            domains.get_mut(caller_domain_id.object_id())
                                        {
                                            let _ = arena.destroy(domain, mapping_id);
                                        }
                                    }
                                    break 'sys_invoke u64::MAX;
                                }
                            }
                            drop(mem_objects);

                            let mapping = match kernel_core::mapping::Mapping::try_new_memory_object(
                                mapping_id,
                                aspace_id,
                                virtual_address,
                                mem_id,
                                offset_bytes,
                                map_length as usize,
                                requested_rights,
                                Some(node_id),
                            ) {
                                Ok(m) => m,
                                Err(_) => {
                                    let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
                                    if let Some(mem_obj) = mem_objects.get_mut(mem_id) {
                                        let _ = mem_obj.dec_mapping_ref();
                                    }
                                    drop(mem_objects);
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace.arch.unmap_range(
                                            virtual_address,
                                            (map_length / 4096) as usize,
                                        );
                                    }
                                    drop(aspaces);
                                    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                                    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                                    if let Some(arena) = arena_guard.as_mut() {
                                        if let Some(domain) =
                                            domains.get_mut(caller_domain_id.object_id())
                                        {
                                            let _ = arena.destroy(domain, mapping_id);
                                        }
                                    }
                                    break 'sys_invoke u64::MAX;
                                }
                            };
                            let mut mappings = crate::global::MAPPINGS.lock();
                            mappings.insert(mapping_id, mapping);
                            drop(mappings);

                            // Transaction step 4: Insert capability handle
                            let mut domain_guard = crate::global::RESOURCE_DOMAINS.lock();
                            let domain = match domain_guard.get_mut(caller_domain_id.object_id()) {
                                Some(d) => d,
                                None => break 'sys_invoke u64::MAX,
                            };

                            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                            let arena = match arena_guard.as_mut() {
                                Some(a) => a,
                                None => break 'sys_invoke u64::MAX,
                            };

                            let mut cspaces_guard = crate::global::CAPABILITY_SPACES.lock();
                            let cspace = match cspaces_guard.get_mut(cspace_id) {
                                Some(c) => c,
                                None => break 'sys_invoke u64::MAX,
                            };

                            let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
                            let sys = match system_guard.as_mut() {
                                Some(s) => s,
                                None => break 'sys_invoke u64::MAX,
                            };

                            match sys.insert_descendant(
                                node_id,
                                cspace,
                                domain,
                                mapping_id,
                                gaxera_abi::ObjectType::Mapping,
                                // A Mapping retains MAP authority so its owner can
                                // explicitly unmap it. Page permissions remain the
                                // requested subset stored in the Mapping object.
                                requested_rights | gaxera_abi::Rights::MAP,
                                arena,
                            ) {
                                Ok(h) => {
                                    frame.rdx = h.raw();
                                }
                                Err(_) => {
                                    drop(system_guard);
                                    drop(cspaces_guard);
                                    drop(arena_guard);
                                    drop(domain_guard);
                                    let mut mappings = crate::global::MAPPINGS.lock();
                                    let _ = mappings.remove(mapping_id);
                                    drop(mappings);
                                    let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
                                    if let Some(mem_obj) = mem_objects.get_mut(mem_id) {
                                        let _ = mem_obj.dec_mapping_ref();
                                    }
                                    drop(mem_objects);
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace.arch.unmap_range(
                                            virtual_address,
                                            (map_length / 4096) as usize,
                                        );
                                    }
                                    drop(aspaces);
                                    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                                    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                                    if let Some(arena) = arena_guard.as_mut() {
                                        if let Some(domain) =
                                            domains.get_mut(caller_domain_id.object_id())
                                        {
                                            let _ = arena.destroy(domain, mapping_id);
                                        }
                                    }
                                    break 'sys_invoke u64::MAX;
                                }
                            }
                            0
                        } else if let Ok(mapping_id) = mapping_result {
                            let source_node = match sys.node_for(cspace, mem_handle) {
                                Ok(node) => node,
                                Err(_) => break 'sys_invoke u64::MAX,
                            };
                            if !mapping_rights
                                .is_ok_and(|rights| requested_rights.is_subset_of(rights))
                            {
                                break 'sys_invoke u64::MAX;
                            }
                            drop(arena);
                            drop(system);
                            drop(cspaces);

                            let mappings = crate::global::MAPPINGS.lock();
                            let mapping = match mappings.get(mapping_id) {
                                Some(m) => m,
                                None => break 'sys_invoke u64::MAX,
                            };

                            let size = mapping.size();
                            let is_valid_user_range = virtual_address
                                .checked_add(size as u64)
                                .is_some_and(|end_vaddr| end_vaddr <= USER_ADDRESS_MAX);

                            if !is_valid_user_range {
                                break 'sys_invoke u64::MAX;
                            }

                            let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                            let aspace = match aspaces.get_mut(aspace_id) {
                                Some(a) => a,
                                None => break 'sys_invoke u64::MAX,
                            };

                            let phys_base = if let Some(phys_base) = mapping.phys_addr() {
                                phys_base
                            } else {
                                crate::println!("GAXERA: MapMemory phys_addr is None!");
                                break 'sys_invoke u64::MAX;
                            };
                            let map_size = mapping.size();
                            let cache_policy = mapping.cache_policy();
                            drop(mappings);
                            if aspace
                                .arch
                                .map_physical_range(
                                    virtual_address,
                                    phys_base,
                                    map_size,
                                    requested_rights,
                                    cache_policy,
                                )
                                .is_err()
                            {
                                break 'sys_invoke u64::MAX;
                            }
                            drop(aspaces);

                            let caller_domain_id = {
                                let cspaces = crate::global::CAPABILITY_SPACES.lock();
                                cspaces
                                    .get(cspace_id)
                                    .map(|cspace| cspace.domain())
                                    .ok_or(())
                            };
                            let caller_domain_id = match caller_domain_id {
                                Ok(id) => id,
                                Err(_) => break 'sys_invoke u64::MAX,
                            };
                            let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                            let domain = match domains.get_mut(caller_domain_id.object_id()) {
                                Some(domain) => domain,
                                None => {
                                    drop(domains);
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_size / 4096);
                                    }
                                    break 'sys_invoke u64::MAX;
                                }
                            };
                            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                            let arena = match arena_guard.as_mut() {
                                Some(arena) => arena,
                                None => break 'sys_invoke u64::MAX,
                            };
                            let new_mapping_id = match arena.create_mapping(domain) {
                                Ok(id) => id,
                                Err(_) => {
                                    drop(arena_guard);
                                    drop(domains);
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_size / 4096);
                                    }
                                    break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                }
                            };
                            let new_mapping = match kernel_core::mapping::Mapping::try_new_mmio(
                                new_mapping_id,
                                aspace_id,
                                virtual_address,
                                phys_base,
                                map_size,
                                cache_policy,
                                requested_rights,
                            ) {
                                Ok(mapping) => mapping,
                                Err(_) => {
                                    let _ = arena.destroy(domain, new_mapping_id);
                                    drop(arena_guard);
                                    drop(domains);
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_size / 4096);
                                    }
                                    break 'sys_invoke u64::MAX;
                                }
                            };
                            drop(arena_guard);
                            drop(domains);
                            crate::global::MAPPINGS
                                .lock()
                                .insert(new_mapping_id, new_mapping);

                            let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                            let domain = match domains.get_mut(caller_domain_id.object_id()) {
                                Some(domain) => domain,
                                None => break 'sys_invoke u64::MAX,
                            };
                            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                            let arena = match arena_guard.as_mut() {
                                Some(arena) => arena,
                                None => break 'sys_invoke u64::MAX,
                            };
                            let mut cspaces_guard = crate::global::CAPABILITY_SPACES.lock();
                            let cspace = match cspaces_guard.get_mut(cspace_id) {
                                Some(cspace) => cspace,
                                None => break 'sys_invoke u64::MAX,
                            };
                            let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
                            let system = match system_guard.as_mut() {
                                Some(system) => system,
                                None => break 'sys_invoke u64::MAX,
                            };
                            match system.insert_descendant(
                                source_node,
                                cspace,
                                domain,
                                new_mapping_id,
                                gaxera_abi::ObjectType::Mapping,
                                requested_rights | gaxera_abi::Rights::MAP,
                                arena,
                            ) {
                                Ok(handle) => {
                                    frame.rdx = handle.raw();
                                    0
                                }
                                Err(_) => {
                                    drop(system_guard);
                                    drop(cspaces_guard);
                                    drop(arena_guard);
                                    drop(domains);
                                    let mut mappings = crate::global::MAPPINGS.lock();
                                    let _ = mappings.remove(new_mapping_id);
                                    drop(mappings);
                                    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                                    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                                    if let Some(arena) = arena_guard.as_mut()
                                        && let Some(domain) =
                                            domains.get_mut(caller_domain_id.object_id())
                                    {
                                        let _ = arena.destroy(domain, new_mapping_id);
                                    }
                                    drop(arena_guard);
                                    drop(domains);
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_size / 4096);
                                    }
                                    break 'sys_invoke gaxera_abi::status::CAPABILITY_LIMIT;
                                }
                            }
                        } else if let Ok(frame_obj_id) = frame_result {
                            let node_id = match sys.node_for(cspace, mem_handle) {
                                Ok(n) => n,
                                Err(_) => break 'sys_invoke u64::MAX,
                            };
                            let caller_domain_id = cspace.domain();
                            drop(arena);
                            drop(system);
                            drop(cspaces);

                            let (physical_base, frame_size) = {
                                let frames = crate::global::CONTIGUOUS_FRAMES.lock();
                                let frame_obj = match frames.get(frame_obj_id) {
                                    Some(frame_obj) => frame_obj,
                                    None => break 'sys_invoke u64::MAX,
                                };
                                let size = frame_obj.page_count().checked_mul(4096);
                                match size {
                                    Some(size) => (frame_obj.base_frame(), size),
                                    None => break 'sys_invoke u64::MAX,
                                }
                            };
                            let map_length = if length_bytes == 0 {
                                frame_size.saturating_sub(offset_bytes as usize)
                            } else {
                                length_bytes as usize
                            };
                            if map_length == 0
                                || (offset_bytes & 0xFFF) != 0
                                || (map_length & 0xFFF) != 0
                                || offset_bytes as usize > frame_size
                                || offset_bytes as usize + map_length > frame_size
                            {
                                break 'sys_invoke gaxera_abi::status::INVALID_ARGUMENT;
                            }
                            let physical = match physical_base.checked_add(offset_bytes) {
                                Some(physical) => physical,
                                None => break 'sys_invoke gaxera_abi::status::INVALID_ARGUMENT,
                            };

                            let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                            let aspace = match aspaces.get_mut(aspace_id) {
                                Some(aspace) => aspace,
                                None => break 'sys_invoke gaxera_abi::status::INVALID_HANDLE,
                            };
                            use kernel_core::address_space::ArchAddressSpace;
                            if aspace
                                .arch
                                .map_physical_range(
                                    virtual_address,
                                    physical,
                                    map_length,
                                    requested_rights,
                                    gaxera_abi::CachePolicy::Cached,
                                )
                                .is_err()
                            {
                                break 'sys_invoke gaxera_abi::status::MAPPING_COLLISION;
                            }
                            drop(aspaces);
                            if let Some(frame_obj) = crate::global::CONTIGUOUS_FRAMES
                                .lock()
                                .get_mut(frame_obj_id)
                            {
                                if frame_obj.add_mapping().is_err() {
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_length / 4096);
                                    }
                                    break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                }
                            } else {
                                let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                    let _ =
                                        aspace.arch.unmap_range(virtual_address, map_length / 4096);
                                }
                                break 'sys_invoke gaxera_abi::status::INVALID_HANDLE;
                            }

                            let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                            let domain = match domains.get_mut(caller_domain_id.object_id()) {
                                Some(domain) => domain,
                                None => {
                                    drop(domains);
                                    let _ = crate::global::CONTIGUOUS_FRAMES
                                        .lock()
                                        .get_mut(frame_obj_id)
                                        .and_then(|frame_obj| frame_obj.remove_mapping().ok());
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_length / 4096);
                                    }
                                    break 'sys_invoke gaxera_abi::status::INVALID_HANDLE;
                                }
                            };
                            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                            let arena = match arena_guard.as_mut() {
                                Some(arena) => arena,
                                None => {
                                    drop(arena_guard);
                                    drop(domains);
                                    let _ = crate::global::CONTIGUOUS_FRAMES
                                        .lock()
                                        .get_mut(frame_obj_id)
                                        .and_then(|frame_obj| frame_obj.remove_mapping().ok());
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_length / 4096);
                                    }
                                    break 'sys_invoke gaxera_abi::status::INTERNAL_ERROR;
                                }
                            };
                            let mapping_id = match arena.create_mapping(domain) {
                                Ok(mapping_id) => mapping_id,
                                Err(_) => {
                                    drop(arena_guard);
                                    drop(domains);
                                    let _ = crate::global::CONTIGUOUS_FRAMES
                                        .lock()
                                        .get_mut(frame_obj_id)
                                        .and_then(|frame_obj| frame_obj.remove_mapping().ok());
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_length / 4096);
                                    }
                                    break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                }
                            };
                            let mapping =
                                match kernel_core::mapping::Mapping::try_new_contiguous_frame(
                                    mapping_id,
                                    aspace_id,
                                    virtual_address,
                                    frame_obj_id,
                                    physical_base,
                                    offset_bytes,
                                    map_length,
                                    requested_rights,
                                ) {
                                    Ok(mapping) => mapping,
                                    Err(_) => {
                                        let _ = arena.destroy(domain, mapping_id);
                                        drop(arena_guard);
                                        drop(domains);
                                        let _ = crate::global::CONTIGUOUS_FRAMES
                                            .lock()
                                            .get_mut(frame_obj_id)
                                            .and_then(|frame_obj| frame_obj.remove_mapping().ok());
                                        let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                        if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                            let _ = aspace
                                                .arch
                                                .unmap_range(virtual_address, map_length / 4096);
                                        }
                                        break 'sys_invoke gaxera_abi::status::INVALID_ARGUMENT;
                                    }
                                };
                            drop(arena_guard);
                            drop(domains);
                            crate::global::MAPPINGS.lock().insert(mapping_id, mapping);

                            let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                            let domain = match domains.get_mut(caller_domain_id.object_id()) {
                                Some(domain) => domain,
                                None => break 'sys_invoke gaxera_abi::status::INVALID_HANDLE,
                            };
                            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                            let arena = match arena_guard.as_mut() {
                                Some(arena) => arena,
                                None => break 'sys_invoke gaxera_abi::status::INTERNAL_ERROR,
                            };
                            let mut cspaces_guard = crate::global::CAPABILITY_SPACES.lock();
                            let cspace = match cspaces_guard.get_mut(cspace_id) {
                                Some(cspace) => cspace,
                                None => break 'sys_invoke gaxera_abi::status::INVALID_HANDLE,
                            };
                            let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
                            let system = match system_guard.as_mut() {
                                Some(system) => system,
                                None => break 'sys_invoke gaxera_abi::status::INTERNAL_ERROR,
                            };
                            match system.insert_descendant(
                                node_id,
                                cspace,
                                domain,
                                mapping_id,
                                gaxera_abi::ObjectType::Mapping,
                                requested_rights | gaxera_abi::Rights::MAP,
                                arena,
                            ) {
                                Ok(handle) => {
                                    frame.rdx = handle.raw();
                                    0
                                }
                                Err(_) => {
                                    drop(system_guard);
                                    drop(cspaces_guard);
                                    drop(arena_guard);
                                    drop(domains);
                                    let _ = crate::global::MAPPINGS.lock().remove(mapping_id);
                                    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                                    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                                    if let Some(arena) = arena_guard.as_mut()
                                        && let Some(domain) =
                                            domains.get_mut(caller_domain_id.object_id())
                                    {
                                        let _ = arena.destroy(domain, mapping_id);
                                    }
                                    drop(arena_guard);
                                    drop(domains);
                                    let _ = crate::global::CONTIGUOUS_FRAMES
                                        .lock()
                                        .get_mut(frame_obj_id)
                                        .and_then(|frame_obj| frame_obj.remove_mapping().ok());
                                    let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                                    if let Some(aspace) = aspaces.get_mut(aspace_id) {
                                        let _ = aspace
                                            .arch
                                            .unmap_range(virtual_address, map_length / 4096);
                                    }
                                    break 'sys_invoke gaxera_abi::status::CAPABILITY_LIMIT;
                                }
                            }
                        } else {
                            break 'sys_invoke u64::MAX;
                        }
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::UnmapMemory as u64 {
                    let mapping_handle = handle;
                    let caller_domain_id = cspace.domain();
                    let mapping_result = sys.lookup(
                        cspace,
                        mapping_handle,
                        gaxera_abi::ObjectType::Mapping,
                        gaxera_abi::Rights::MAP,
                        arena_ref,
                    );

                    if let Ok(mapping_id) = mapping_result {
                        drop(arena);
                        drop(system);
                        drop(cspaces);

                        let mut mappings = crate::global::MAPPINGS.lock();
                        let (vaddr, size, aspace_id, backing) = match mappings.get_mut(mapping_id) {
                            Some(m) if !m.is_closed() => {
                                let _ = m.close();
                                (
                                    m.virtual_address(),
                                    m.size(),
                                    m.target_address_space(),
                                    m.backing().clone(),
                                )
                            }
                            _ => break 'sys_invoke u64::MAX,
                        };
                        drop(mappings);

                        // 1. Unmap page tables FIRST
                        let mut aspaces = crate::global::ADDRESS_SPACES.lock();
                        let aspace = match aspaces.get_mut(aspace_id) {
                            Some(a) => a,
                            None => break 'sys_invoke u64::MAX,
                        };

                        use kernel_core::address_space::ArchAddressSpace;
                        let page_count = size / 4096;
                        if aspace.arch.unmap_range(vaddr, page_count).is_err() {
                            break 'sys_invoke u64::MAX;
                        }
                        drop(aspaces);

                        // 2. Multi-page TLB flush
                        let mut flush_vaddr = vaddr;
                        let end_vaddr = vaddr + size as u64;
                        while flush_vaddr < end_vaddr {
                            // SAFETY: TLB flush for unmapped virtual address.
                            unsafe {
                                x86_64::instructions::tlb::flush(x86_64::VirtAddr::new(
                                    flush_vaddr,
                                ));
                            }
                            flush_vaddr += 4096;
                        }

                        // 3. Remove Mapping from MAPPINGS
                        let mut mappings = crate::global::MAPPINGS.lock();
                        let _ = mappings.remove(mapping_id);
                        drop(mappings);

                        // 4. Delete Mapping handle from CapabilitySpace and OBJECT_ARENA
                        let mut domain_guard = crate::global::RESOURCE_DOMAINS.lock();
                        if let Some(domain) = domain_guard.get_mut(caller_domain_id.object_id()) {
                            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                            if let Some(arena) = arena_guard.as_mut() {
                                let mut cspaces_guard = crate::global::CAPABILITY_SPACES.lock();
                                if let Some(cspace) = cspaces_guard.get_mut(cspace_id) {
                                    let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
                                    if let Some(sys) = system_guard.as_mut() {
                                        let _ = sys.delete(cspace, domain, mapping_handle);
                                    }
                                }
                                let _ = arena.destroy(domain, mapping_id);
                            }
                        }
                        drop(domain_guard);

                        // 5. Decrement MemoryObject mapping reference and reclaim if zero refs
                        if let kernel_core::mapping::MappingBacking::MemoryObject {
                            object_id,
                            ..
                        } = backing
                        {
                            let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
                            let can_destroy = if let Some(mem_obj) = mem_objects.get_mut(object_id)
                            {
                                mem_obj.dec_mapping_ref().unwrap_or(false)
                            } else {
                                false
                            };
                            drop(mem_objects);

                            if can_destroy {
                                reclaim_memory_object_if_zero_refs(object_id);
                            }
                        } else if let kernel_core::mapping::MappingBacking::ContiguousFrame {
                            object_id,
                            ..
                        } = backing
                        {
                            let can_destroy = {
                                let mut frames = crate::global::CONTIGUOUS_FRAMES.lock();
                                frames.get_mut(object_id).map(|frame_obj| {
                                    if frame_obj.remove_mapping().is_ok() {
                                        frame_obj.can_destroy()
                                    } else {
                                        false
                                    }
                                })
                            }
                            .unwrap_or(false);
                            if can_destroy {
                                reclaim_contiguous_frame_if_zero_refs(object_id);
                            }
                        }

                        frame.rax = 0;
                        0
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::CreateWaitSet as u64 {
                    let mut domain_guard = crate::global::RESOURCE_DOMAINS.lock();
                    let domain = match domain_guard.iter_mut().next().map(|(_, d)| d) {
                        Some(d) => d,
                        None => break 'sys_invoke u64::MAX,
                    };

                    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
                    let arena = match arena_guard.as_mut() {
                        Some(a) => a,
                        None => break 'sys_invoke u64::MAX,
                    };

                    let ws_id = match arena.create_waitset(domain) {
                        Ok(id) => id,
                        Err(_) => break 'sys_invoke u64::MAX,
                    };

                    let ws = kernel_core::waitset::WaitSet::new(ws_id);
                    crate::global::WAIT_SETS.lock().insert(ws_id, ws);

                    match sys.insert_root(
                        cspace,
                        domain,
                        ws_id,
                        gaxera_abi::ObjectType::WaitSet,
                        gaxera_abi::Rights::ALL,
                        arena,
                    ) {
                        Ok(h) => h.raw(),
                        Err(_) => {
                            crate::global::WAIT_SETS.lock().remove(ws_id);
                            let _ = arena.destroy(domain, ws_id);
                            u64::MAX
                        }
                    }
                } else if op == gaxera_abi::OperationCode::WaitSetControl as u64 {
                    let ws_res = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::WaitSet,
                        gaxera_abi::Rights::WRITE,
                        arena_ref,
                    );
                    let target_handle = gaxera_abi::Handle::from_raw(frame.rdx);
                    let ctrl_op = frame.r10;
                    let cookie = frame.r8;
                    let signals = frame.r9 as u32;

                    let target_id = match sys
                        .lookup(
                            cspace,
                            target_handle,
                            gaxera_abi::ObjectType::Endpoint,
                            gaxera_abi::Rights::NONE,
                            arena_ref,
                        )
                        .or_else(|_| {
                            sys.lookup(
                                cspace,
                                target_handle,
                                gaxera_abi::ObjectType::Notification,
                                gaxera_abi::Rights::NONE,
                                arena_ref,
                            )
                        })
                        .or_else(|_| {
                            sys.lookup(
                                cspace,
                                target_handle,
                                gaxera_abi::ObjectType::TimerObject,
                                gaxera_abi::Rights::NONE,
                                arena_ref,
                            )
                        }) {
                        Ok(id) => id,
                        Err(_) => break 'sys_invoke u64::MAX,
                    };

                    if let Ok(ws_id) = ws_res {
                        drop(arena);
                        drop(system);
                        drop(cspaces);

                        let mut wsets = crate::global::WAIT_SETS.lock();
                        let ws = match wsets.get_mut(ws_id) {
                            Some(w) => w,
                            None => break 'sys_invoke u64::MAX,
                        };

                        if ctrl_op == gaxera_abi::WaitSetOp::Add as u64 {
                            match ws.add_subscription(target_id, cookie, signals) {
                                Ok(_) => 0,
                                Err(_) => u64::MAX,
                            }
                        } else if ctrl_op == gaxera_abi::WaitSetOp::Remove as u64 {
                            match ws.remove_subscription(target_id) {
                                Ok(_) => 0,
                                Err(_) => u64::MAX,
                            }
                        } else {
                            u64::MAX
                        }
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::WaitSetWait as u64 {
                    let ws_res = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::WaitSet,
                        gaxera_abi::Rights::READ,
                        arena_ref,
                    );
                    if let Ok(ws_id) = ws_res {
                        drop(arena);
                        drop(system);
                        drop(cspaces);

                        let mut wsets = crate::global::WAIT_SETS.lock();
                        let ws = match wsets.get_mut(ws_id) {
                            Some(w) => w,
                            None => break 'sys_invoke u64::MAX,
                        };

                        match ws.wait(current_thread_id) {
                            Ok(Ok(events)) => events.len() as u64,
                            Ok(Err(_)) => {
                                drop(wsets);
                                // SAFETY: Single core BSP, no data races.
                                let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
                                let scheduler = scheduler_cell.as_mut().unwrap();

                                // SAFETY: Thread exists and is accessed exclusively by scheduler.
                                let thread = unsafe {
                                    crate::arch::x86_64::thread::THREADS.get_mut(current_thread_id)
                                }
                                .unwrap();
                                let _ = scheduler.block_current(thread);
                                if let Some(next) = scheduler.dequeue_next() {
                                    scheduler.set_current_thread(Some(next));
                                    let _ = crate::arch::x86_64::preemption::switch_to_next(
                                        current_thread_id,
                                        next,
                                    );
                                }
                                0
                            }
                            Err(_) => u64::MAX,
                        }
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::Call as u64 {
                    let endpoint_result = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::Endpoint,
                        gaxera_abi::Rights::WRITE,
                        arena_ref,
                    );
                    if let Ok(endpoint_id) = endpoint_result {
                        let mut payload = [0u8; 32];
                        payload[0..8].copy_from_slice(&frame.rdx.to_le_bytes());
                        payload[8..16].copy_from_slice(&frame.r10.to_le_bytes());
                        payload[16..24].copy_from_slice(&frame.r8.to_le_bytes());
                        payload[24..32].copy_from_slice(&frame.r9.to_le_bytes());
                        let message = match gaxera_abi::ipc::InlineMessage::try_new(&payload) {
                            Ok(m) => m,
                            Err(_) => break 'sys_invoke u64::MAX,
                        };

                        // Fetch caller's effective priority for priority inheritance
                        // SAFETY: Thread access is single-CPU isolated during syscall context.
                        let caller_prio = unsafe {
                            crate::arch::x86_64::thread::THREADS
                                .get(current_thread_id)
                                .map(|t| t.effective_priority())
                                .unwrap_or(0)
                        };

                        drop(arena);
                        drop(system);
                        drop(cspaces);

                        let mut endpoints = crate::global::ENDPOINTS.lock();
                        let endpoint = match endpoints.get_mut(endpoint_id) {
                            Some(e) => e,
                            None => break 'sys_invoke u64::MAX,
                        };
                        let call_result = endpoint.call(current_thread_id, message);
                        drop(endpoints);

                        // SAFETY: Single core BSP, no data races.
                        let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
                        let scheduler = scheduler_cell.as_mut().unwrap();

                        match call_result {
                            Ok(kernel_core::ipc::IpcEffect::Block) => {
                                // SAFETY: Thread exists and is accessed exclusively by scheduler.
                                let thread = unsafe {
                                    crate::arch::x86_64::thread::THREADS.get_mut(current_thread_id)
                                }
                                .unwrap();
                                let _ = scheduler.block_current(thread);
                                if let Some(next) = scheduler.dequeue_next() {
                                    scheduler.set_current_thread(Some(next));
                                    crate::arch::x86_64::preemption::switch_to_next(
                                        current_thread_id,
                                        next,
                                    )
                                    .unwrap();
                                }
                            }
                            Ok(kernel_core::ipc::IpcEffect::Wake(receiver_id)) => {
                                // Block caller (ourselves) because we are waiting for a reply
                                // SAFETY: Thread access is single-CPU isolated during syscall context.
                                let thread = unsafe {
                                    crate::arch::x86_64::thread::THREADS.get_mut(current_thread_id)
                                }
                                .unwrap();
                                let _ = scheduler.block_current(thread);

                                // Boost receiver server thread priority to caller's priority
                                // SAFETY: Receiver exists and access is mutually exclusive.
                                let receiver = unsafe {
                                    crate::arch::x86_64::thread::THREADS.get_mut(receiver_id)
                                }
                                .unwrap();
                                receiver.boost_priority(caller_prio);
                                let _ = scheduler.apply_wake(receiver);

                                // Dequeue highest priority ready thread and switch to it
                                if let Some(next) = scheduler.dequeue_next() {
                                    scheduler.set_current_thread(Some(next));
                                    crate::arch::x86_64::preemption::switch_to_next(
                                        current_thread_id,
                                        next,
                                    )
                                    .unwrap();
                                }
                            }
                            Err(_) => break 'sys_invoke u64::MAX,
                        }

                        // Woken up! Fetch reply
                        // SAFETY: Thread access is single-CPU isolated during syscall context.
                        let thread = unsafe {
                            crate::arch::x86_64::thread::THREADS.get_mut(current_thread_id)
                        }
                        .unwrap();
                        if let Some(reply) = thread.ipc_receive_buffer.take() {
                            let payload = reply.payload();
                            if payload.len() >= 8 {
                                frame.rdx = u64::from_le_bytes(payload[0..8].try_into().unwrap());
                            }
                            if payload.len() >= 16 {
                                frame.r10 = u64::from_le_bytes(payload[8..16].try_into().unwrap());
                            }
                            if payload.len() >= 24 {
                                frame.r8 = u64::from_le_bytes(payload[16..24].try_into().unwrap());
                            }
                            if payload.len() >= 32 {
                                frame.r9 = u64::from_le_bytes(payload[24..32].try_into().unwrap());
                            }
                        }
                        0
                    } else if let Ok(frame_obj_id) = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::ContiguousFrame,
                        gaxera_abi::Rights::READ,
                        arena_ref,
                    ) {
                        drop(arena);
                        drop(system);
                        drop(cspaces);

                        let frame_objs = crate::global::CONTIGUOUS_FRAMES.lock();
                        if let Some(obj) = frame_objs.get(frame_obj_id) {
                            frame.rdx = obj.base_frame();
                            frame.r10 = (obj.page_count() * 4096) as u64;
                            0
                        } else {
                            u64::MAX
                        }
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::Receive as u64 {
                    let endpoint_result = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::Endpoint,
                        gaxera_abi::Rights::READ,
                        arena_ref,
                    );
                    if let Ok(endpoint_id) = endpoint_result {
                        drop(arena);
                        drop(system);
                        drop(cspaces);

                        let mut endpoints = crate::global::ENDPOINTS.lock();
                        let endpoint = match endpoints.get_mut(endpoint_id) {
                            Some(e) => e,
                            None => break 'sys_invoke u64::MAX,
                        };
                        let recv_result = endpoint.receive(current_thread_id);
                        drop(endpoints);

                        match recv_result {
                            Ok(Ok(call)) => {
                                // Boost server priority to popped caller's priority
                                // SAFETY: Thread access is single-CPU isolated during syscall context.
                                let caller_prio = unsafe {
                                    crate::arch::x86_64::thread::THREADS
                                        .get(call.caller)
                                        .map(|t| t.effective_priority())
                                        .unwrap_or(0)
                                };
                                // SAFETY: Thread access is single-CPU isolated during syscall context.
                                let server = unsafe {
                                    crate::arch::x86_64::thread::THREADS.get_mut(current_thread_id)
                                }
                                .unwrap();
                                server.boost_priority(caller_prio);

                                frame.rdi = call.reply_token.raw();
                                frame.rsi = gaxera_abi::Handle::from_parts(
                                    call.caller.index(),
                                    call.caller.generation(),
                                )
                                .raw();
                                let payload = call.message.payload();
                                if payload.len() >= 8 {
                                    frame.rdx =
                                        u64::from_le_bytes(payload[0..8].try_into().unwrap());
                                }
                                if payload.len() >= 16 {
                                    frame.r10 =
                                        u64::from_le_bytes(payload[8..16].try_into().unwrap());
                                }
                                if payload.len() >= 24 {
                                    frame.r8 =
                                        u64::from_le_bytes(payload[16..24].try_into().unwrap());
                                }
                                if payload.len() >= 32 {
                                    frame.r9 =
                                        u64::from_le_bytes(payload[24..32].try_into().unwrap());
                                }
                                0
                            }
                            Ok(Err(kernel_core::ipc::IpcEffect::Block)) => {
                                // SAFETY: Single core BSP, no data races.
                                let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
                                let scheduler = scheduler_cell.as_mut().unwrap();

                                // SAFETY: Thread access is single-CPU isolated during syscall context.
                                let thread = unsafe {
                                    crate::arch::x86_64::thread::THREADS.get_mut(current_thread_id)
                                }
                                .unwrap();
                                thread.restore_priority();
                                let _ = scheduler.block_current(thread);
                                if let Some(next) = scheduler.dequeue_next() {
                                    scheduler.set_current_thread(Some(next));
                                    crate::arch::x86_64::preemption::switch_to_next(
                                        current_thread_id,
                                        next,
                                    )
                                    .unwrap();
                                }

                                // Woken up! Message must be in endpoint
                                let mut endpoints = crate::global::ENDPOINTS.lock();
                                if let Some(call) = endpoints
                                    .get_mut(endpoint_id)
                                    .and_then(|e| e.take_received_call())
                                {
                                    // SAFETY: Thread access is single-CPU isolated during syscall context.
                                    let caller_prio = unsafe {
                                        crate::arch::x86_64::thread::THREADS
                                            .get(call.caller)
                                            .map(|t| t.effective_priority())
                                            .unwrap_or(0)
                                    };
                                    // SAFETY: Thread access is single-CPU isolated during syscall context.
                                    let server = unsafe {
                                        crate::arch::x86_64::thread::THREADS
                                            .get_mut(current_thread_id)
                                    }
                                    .unwrap();
                                    server.boost_priority(caller_prio);

                                    frame.rdi = call.reply_token.raw();
                                    frame.rsi = gaxera_abi::Handle::from_parts(
                                        call.caller.index(),
                                        call.caller.generation(),
                                    )
                                    .raw();
                                    let payload = call.message.payload();
                                    if payload.len() >= 8 {
                                        frame.rdx =
                                            u64::from_le_bytes(payload[0..8].try_into().unwrap());
                                    }
                                    if payload.len() >= 16 {
                                        frame.r10 =
                                            u64::from_le_bytes(payload[8..16].try_into().unwrap());
                                    }
                                    if payload.len() >= 24 {
                                        frame.r8 =
                                            u64::from_le_bytes(payload[16..24].try_into().unwrap());
                                    }
                                    if payload.len() >= 32 {
                                        frame.r9 =
                                            u64::from_le_bytes(payload[24..32].try_into().unwrap());
                                    }
                                }
                                0
                            }
                            _ => u64::MAX,
                        }
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::Reply as u64 {
                    let mut payload = [0u8; 32];
                    payload[0..8].copy_from_slice(&frame.rdx.to_le_bytes());
                    payload[8..16].copy_from_slice(&frame.r10.to_le_bytes());
                    payload[16..24].copy_from_slice(&frame.r8.to_le_bytes());
                    payload[24..32].copy_from_slice(&frame.r9.to_le_bytes());
                    let message = match gaxera_abi::ipc::InlineMessage::try_new(&payload) {
                        Ok(m) => m,
                        Err(_) => break 'sys_invoke u64::MAX,
                    };
                    let reply_token = gaxera_abi::ipc::ReplyToken::from_raw(frame.rdi);

                    let mut valid_reply = false;
                    let mut woken_caller_id = None;
                    let mut ep_id_opt = None;
                    {
                        let mut endpoints = crate::global::ENDPOINTS.lock();
                        for (id, ep) in endpoints.iter_mut() {
                            if let Ok(kernel_core::ipc::IpcEffect::Wake(woken_id)) =
                                ep.reply(reply_token, message)
                            {
                                valid_reply = true;
                                woken_caller_id = Some(woken_id);
                                ep_id_opt = Some(id);
                                break;
                            }
                        }
                    }

                    drop(arena);
                    drop(system);
                    drop(cspaces);

                    if !valid_reply || woken_caller_id.is_none() {
                        break 'sys_invoke u64::MAX;
                    }

                    let caller_id = woken_caller_id.unwrap();

                    // SAFETY: Single core BSP, no data races.
                    let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
                    let scheduler = scheduler_cell.as_mut().unwrap();

                    // Woken up caller! Fetch caller and apply wake.
                    // SAFETY: The thread map is globally accessible and this scope holds logical exclusion.
                    let caller_thread =
                        unsafe { crate::arch::x86_64::thread::THREADS.get_mut(caller_id) };
                    if let Some(caller) = caller_thread {
                        caller.ipc_receive_buffer = Some(message);
                        let _ = scheduler.apply_wake(caller);
                    }

                    // Check if endpoint has pending callers for atomic priority handoff
                    let mut has_pending_callers = false;
                    if let Some(ep_id) = ep_id_opt {
                        let endpoints = crate::global::ENDPOINTS.lock();
                        if let Some(ep) = endpoints.get(ep_id)
                            && ep.pending_caller_count() > 0
                        {
                            has_pending_callers = true;
                        }
                    }

                    // SAFETY: Thread access is single-CPU isolated during syscall context.
                    let server_thread = unsafe {
                        crate::arch::x86_64::thread::THREADS
                            .get_mut(current_thread_id)
                            .unwrap()
                    };

                    if !has_pending_callers {
                        server_thread.restore_priority();
                    }

                    crate::arch::x86_64::preemption::reschedule(
                        scheduler,
                        current_thread_id,
                        caller_id,
                    )
                    .unwrap();
                    0
                } else if op == gaxera_abi::OperationCode::ConfigureThread as u64 {
                    let thread_result = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::Thread,
                        gaxera_abi::Rights::MANAGE,
                        arena_ref,
                    );
                    if let Ok(thread_id) = thread_result {
                        let rip = frame.rdx; // arg1
                        let rsp = frame.r10; // arg2
                        let aspace_handle = gaxera_abi::Handle::from_raw(frame.r8); // arg3
                        let cspace_handle = gaxera_abi::Handle::from_raw(frame.r9); // arg4

                        // Enforce non-zero lower-half canonical user addresses for thread RIP & RSP
                        if !is_user_return_address(rip) || !is_user_return_address(rsp) {
                            break 'sys_invoke u64::MAX;
                        }

                        let aspace_id = sys.lookup(
                            cspace,
                            aspace_handle,
                            gaxera_abi::ObjectType::AddressSpace,
                            gaxera_abi::Rights::NONE,
                            arena_ref,
                        );
                        let cspace_obj_id = sys.lookup(
                            cspace,
                            cspace_handle,
                            gaxera_abi::ObjectType::CapabilitySpace,
                            gaxera_abi::Rights::NONE,
                            arena_ref,
                        );

                        if let (Ok(a_id), Ok(c_id)) = (aspace_id, cspace_obj_id) {
                            let aspaces = crate::global::ADDRESS_SPACES.lock();
                            let a = aspaces.get(a_id).unwrap();
                            use kernel_core::address_space::ArchAddressSpace;
                            let cr3 = a.arch.root_token();
                            drop(aspaces);

                            // SAFETY: thread_id is valid
                            let thread = unsafe {
                                crate::arch::x86_64::thread::THREADS
                                    .get_mut(thread_id)
                                    .unwrap()
                            };
                            thread.set_cspace(c_id);

                            // Initialize kernel stack for thread to return to userspace via syscall_return
                            let stack_top = thread.arch.stack.top().as_mut_ptr::<u8>();
                            // SAFETY: The stack is newly allocated and exclusive to this thread.
                            unsafe {
                                let frame_ptr = stack_top.sub(core::mem::size_of::<
                                    crate::arch::x86_64::syscall::SyscallFrame,
                                >())
                                    as *mut crate::arch::x86_64::syscall::SyscallFrame;
                                core::ptr::write_bytes(frame_ptr, 0, 1); // zero frame

                                (*frame_ptr).rcx = rip;
                                (*frame_ptr).rsp = rsp;
                                (*frame_ptr).r11 = 0x202; // IF | reserved

                                let ret_addr_ptr = (frame_ptr as *mut u64).sub(1);
                                *ret_addr_ptr = syscall_return as *const () as usize as u64;

                                // Context saves 6 registers: rbp, rbx, r12, r13, r14, r15
                                let context_regs_ptr = ret_addr_ptr.sub(6);
                                core::ptr::write_bytes(context_regs_ptr, 0, 6); // zero registers

                                let mut context = crate::arch::x86_64::context::Context::empty();
                                context.rsp = context_regs_ptr as usize as u64;

                                thread.arch.context = context;
                            }
                            thread.arch.cr3 = Some(
                                x86_64::structures::paging::PhysFrame::from_start_address(
                                    x86_64::PhysAddr::new(cr3),
                                )
                                .unwrap(),
                            );

                            // SAFETY: Single-CPU environment, exclusive scheduler access.
                            let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
                            let scheduler = scheduler_cell.as_mut().unwrap();
                            scheduler.enqueue(thread).unwrap();
                            0
                        } else {
                            break 'sys_invoke u64::MAX;
                        }
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::Write as u64 {
                    let console_result = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::DebugConsole,
                        gaxera_abi::Rights::NONE,
                        arena_ref,
                    );
                    if console_result.is_ok() {
                        let mut payload = [0u8; 32];
                        payload[0..8].copy_from_slice(&frame.rdx.to_le_bytes());
                        payload[8..16].copy_from_slice(&frame.r10.to_le_bytes());
                        payload[16..24].copy_from_slice(&frame.r8.to_le_bytes());
                        payload[24..32].copy_from_slice(&frame.r9.to_le_bytes());

                        let len = payload.iter().position(|&c| c == 0).unwrap_or(32);
                        if let Ok(s) = core::str::from_utf8(&payload[..len]) {
                            crate::print!("{}", s);
                        }
                        0
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::Derive as u64 {
                    crate::println!("GAXERA: Syscall Derive started");
                    // Derive(source_handle, target_cspace_handle, rights)
                    let target_cspace_handle = gaxera_abi::Handle::from_raw(frame.rdx);
                    let requested_rights = gaxera_abi::Rights::from_bits(frame.r10 as u32);

                    let target_cspace_id = match sys.lookup(
                        cspace,
                        target_cspace_handle,
                        gaxera_abi::ObjectType::CapabilitySpace,
                        gaxera_abi::Rights::NONE,
                        arena_ref,
                    ) {
                        Ok(id) => id,
                        Err(_) => break 'sys_invoke u64::MAX,
                    };

                    let cspace_ptr = cspace as *const _;

                    // SAFETY: Single-threaded kernel syscall context with CAPABILITY_SPACES lock held.
                    let target_cspace: &mut kernel_core::capability::CapabilitySpace = unsafe {
                        match (*cspaces_ptr).get_mut(target_cspace_id) {
                            Some(cs) => cs,
                            None => break 'sys_invoke u64::MAX,
                        }
                    };

                    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                    let target_ptr = target_cspace as *mut _;

                    let target_domain = match domains.get_mut(target_cspace.domain().object_id()) {
                        Some(d) => d,
                        None => break 'sys_invoke u64::MAX,
                    };

                    // Check if target is a MemoryObject to increment capability refcount
                    let source_mem_id = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::MemoryObject,
                        gaxera_abi::Rights::NONE,
                        arena_ref,
                    );
                    let source_mapping_id = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::Mapping,
                        gaxera_abi::Rights::NONE,
                        arena_ref,
                    );
                    let source_interrupt_id = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::InterruptObject,
                        gaxera_abi::Rights::NONE,
                        arena_ref,
                    );
                    let source_notification_id = sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::Notification,
                        gaxera_abi::Rights::NONE,
                        arena_ref,
                    );

                    // SAFETY: We have verified that the pointers are valid and we hold the global locks.
                    match unsafe {
                        sys.derive(
                            &*cspace_ptr,
                            handle,
                            &mut *target_ptr,
                            target_domain,
                            requested_rights,
                            arena_ref,
                        )
                    } {
                        Ok(new_handle) => {
                            if let Ok(mem_id) = source_mem_id {
                                let mut mem_objects = crate::global::MEMORY_OBJECTS.lock();
                                if let Some(mem_obj) = mem_objects.get_mut(mem_id) {
                                    let _ = mem_obj.inc_capability_ref();
                                }
                            } else if let Ok(frame_id) = sys.lookup(
                                cspace,
                                handle,
                                gaxera_abi::ObjectType::ContiguousFrame,
                                gaxera_abi::Rights::NONE,
                                arena_ref,
                            ) {
                                let mut frames = crate::global::CONTIGUOUS_FRAMES.lock();
                                if let Some(frame_obj) = frames.get_mut(frame_id) {
                                    let _ = frame_obj.inc_capability();
                                }
                            } else if let Ok(mapping_id) = source_mapping_id {
                                let mut mappings = crate::global::MAPPINGS.lock();
                                if let Some(mapping) = mappings.get_mut(mapping_id) {
                                    let _ = mapping.inc_capability_ref();
                                }
                            } else if let Ok(interrupt_id) = source_interrupt_id {
                                let mut interrupts = crate::global::INTERRUPTS.lock();
                                if let Some(interrupt) = interrupts.get_mut(interrupt_id) {
                                    let _ = interrupt.inc_capability_ref();
                                }
                            } else if let Ok(notification_id) = source_notification_id {
                                let mut notifications = crate::global::NOTIFICATIONS.lock();
                                if let Some(notification) = notifications.get_mut(notification_id) {
                                    let _ = notification.inc_capability_ref();
                                }
                            }
                            new_handle.raw()
                        }
                        Err(_) => u64::MAX,
                    }
                } else if op == gaxera_abi::OperationCode::DeleteHandle as u64 {
                    let target_handle = gaxera_abi::Handle::from_raw(frame.rdx);

                    drop(arena);
                    drop(system);
                    drop(cspaces);

                    let delete_result = crate::arch::x86_64::teardown::delete_handle_internal(
                        cspace_id,
                        target_handle,
                    );
                    if delete_result.is_ok() { 0 } else { u64::MAX }
                } else if op == gaxera_abi::OperationCode::Revoke as u64 {
                    crate::println!("GAXERA: Syscall Revoke started");
                    let target_handle = if frame.rdx != 0 {
                        gaxera_abi::Handle::from_raw(frame.rdx)
                    } else {
                        handle
                    };
                    let revoked_node = match sys.node_for(cspace, target_handle) {
                        Ok(n) => n,
                        Err(_) => break 'sys_invoke u64::MAX,
                    };

                    // Drop locks before calling perform_kernel_selective_revoke to avoid re-entrant deadlock
                    drop(system);
                    drop(arena);
                    drop(cspaces);

                    perform_kernel_selective_revoke(revoked_node);

                    // Re-acquire locks for sys.revoke
                    let mut cspaces = crate::global::CAPABILITY_SPACES.lock();
                    let cspace = match cspaces.get_mut(cspace_id) {
                        Some(c) => c,
                        None => break 'sys_invoke u64::MAX,
                    };
                    let mut system = crate::global::CAPABILITY_SYSTEM.lock();
                    let sys = match system.as_mut() {
                        Some(s) => s,
                        None => break 'sys_invoke u64::MAX,
                    };
                    let arena = crate::global::OBJECT_ARENA.lock();
                    let arena_ref = match arena.as_ref() {
                        Some(a) => a,
                        None => break 'sys_invoke u64::MAX,
                    };

                    match sys.revoke(cspace, target_handle, arena_ref) {
                        Ok(_) => 0,
                        Err(_) => u64::MAX,
                    }
                } else if op == gaxera_abi::OperationCode::FactoryCreate as u64 {
                    // Unreachable: FactoryCreate is dispatched before locks are taken.
                    // See early dispatch above sys_result block.
                    unreachable!("FactoryCreate must be dispatched before sys_result lock scope")
                } else if op == gaxera_abi::OperationCode::ThreadStatus as u64 {
                    match sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::Thread,
                        gaxera_abi::Rights::NONE, // Any right can view status
                        arena_ref,
                    ) {
                        Ok(target_thread_id) => {
                            // SAFETY: Single-CPU environment, exclusive thread access.
                            let target = unsafe {
                                crate::arch::x86_64::thread::THREADS.get_mut(target_thread_id)
                            };
                            match target {
                                Some(target) => {
                                    if target.state() == kernel_core::thread::ThreadState::Dead {
                                        gaxera_abi::THREAD_STATE_DEAD
                                    } else {
                                        gaxera_abi::THREAD_STATE_RUNNABLE_OR_RUNNING
                                    }
                                }
                                None => break 'sys_invoke u64::MAX,
                            }
                        }
                        Err(_) => break 'sys_invoke u64::MAX,
                    }
                } else if op == gaxera_abi::OperationCode::InterruptControl as u64 {
                    match sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::InterruptObject,
                        gaxera_abi::Rights::INTERRUPT,
                        arena_ref,
                    ) {
                        Ok(irq_obj_id) => {
                            // The top-level syscall opcode is in %rsi.  The
                            // InterruptControl sub-operation is the first
                            // syscall argument in %rdx; its optional handle
                            // argument is in %r10.
                            let sub_op = frame.rdx;
                            let notification_id = if sub_op
                                == gaxera_abi::InterruptOp::BindNotification as u64
                            {
                                let notif_handle = gaxera_abi::Handle::from_raw(frame.r10);
                                match sys.lookup(
                                    cspace,
                                    notif_handle,
                                    gaxera_abi::ObjectType::Notification,
                                    gaxera_abi::Rights::SIGNAL,
                                    arena_ref,
                                ) {
                                    Ok(id) => Some(id),
                                    Err(_) => break 'sys_invoke gaxera_abi::status::RIGHTS_DENIED,
                                }
                            } else {
                                None
                            };
                            drop(arena);
                            drop(system);
                            drop(cspaces);

                            let mut interrupts = crate::global::INTERRUPTS.lock();
                            let irq_obj = match interrupts.get_mut(irq_obj_id) {
                                Some(obj) => obj,
                                None => break 'sys_invoke gaxera_abi::status::INVALID_HANDLE,
                            };

                            if sub_op == gaxera_abi::InterruptOp::BindNotification as u64 {
                                let notif_id = notification_id.unwrap();
                                if irq_obj.bind_notification(notif_id).is_err() {
                                    gaxera_abi::status::INVALID_ARGUMENT
                                } else {
                                    let lease =
                                        crate::arch::x86_64::interrupts::VectorLease::from_parts(
                                            irq_obj.vector(),
                                            irq_obj.generation(),
                                        );
                                    if crate::arch::x86_64::interrupts::bind(lease, notif_id)
                                        .is_err()
                                    {
                                        let _ = irq_obj.unbind_notification();
                                        gaxera_abi::status::INTERNAL_ERROR
                                    } else {
                                        0
                                    }
                                }
                            } else if sub_op == gaxera_abi::InterruptOp::Mask as u64 {
                                irq_obj.mask();
                                crate::arch::x86_64::ioapic::ioapic_mask_irq(irq_obj.irq());
                                0
                            } else if sub_op == gaxera_abi::InterruptOp::Unmask as u64 {
                                if irq_obj.bound_notification().is_none() || irq_obj.in_flight() {
                                    break 'sys_invoke gaxera_abi::status::INVALID_ARGUMENT;
                                }
                                irq_obj.unmask();
                                if irq_obj.is_masked() {
                                    gaxera_abi::status::INVALID_ARGUMENT
                                } else {
                                    crate::arch::x86_64::ioapic::ioapic_unmask_irq(irq_obj.irq());
                                    0
                                }
                            } else if sub_op == gaxera_abi::InterruptOp::Ack as u64 {
                                if irq_obj.acknowledge().is_err() {
                                    gaxera_abi::status::INVALID_ARGUMENT
                                } else {
                                    // ACK completes the level-triggered delivery
                                    // transaction.  The ISR masks both the
                                    // controller line and the logical object;
                                    // rearming only the IOAPIC would leave the
                                    // object permanently rejecting the next
                                    // delivery in begin_delivery().
                                    irq_obj.unmask();
                                    crate::arch::x86_64::ioapic::ioapic_unmask_irq(irq_obj.irq());
                                    0
                                }
                            } else {
                                gaxera_abi::status::INVALID_ARGUMENT
                            }
                        }
                        Err(_) => break 'sys_invoke u64::MAX,
                    }
                } else if op == gaxera_abi::OperationCode::WaitNotification as u64 {
                    match sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::Notification,
                        gaxera_abi::Rights::READ,
                        arena_ref,
                    ) {
                        Ok(notif_id) => {
                            drop(arena);
                            drop(system);
                            drop(cspaces);

                            let mut notifications = crate::global::NOTIFICATIONS.lock();
                            let notif = match notifications.get_mut(notif_id) {
                                Some(n) => n,
                                None => break 'sys_invoke u64::MAX,
                            };
                            let wait_res = notif.wait(current_thread_id);
                            drop(notifications);

                            match wait_res {
                                Ok(Ok(signals)) => signals as u64,
                                Ok(Err(_thread_id)) => {
                                    // SAFETY: Single core BSP, no data races.
                                    let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
                                    let scheduler = scheduler_cell.as_mut().unwrap();

                                    // SAFETY: Thread access is single-CPU isolated during syscall context.
                                    let thread = unsafe {
                                        crate::arch::x86_64::thread::THREADS
                                            .get_mut(current_thread_id)
                                    }
                                    .unwrap();
                                    let _ = scheduler.block_current(thread);
                                    if let Some(next) = scheduler.dequeue_next() {
                                        scheduler.set_current_thread(Some(next));
                                        crate::arch::x86_64::preemption::switch_to_next(
                                            current_thread_id,
                                            next,
                                        )
                                        .unwrap();
                                    } else {
                                        let Some(idle) = crate::arch::x86_64::thread::idle_thread()
                                        else {
                                            break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                        };
                                        scheduler.set_current_thread(Some(idle));
                                        if crate::arch::x86_64::preemption::switch_to_next(
                                            current_thread_id,
                                            idle,
                                        )
                                        .is_err()
                                        {
                                            break 'sys_invoke gaxera_abi::status::INTERNAL_ERROR;
                                        }
                                    }

                                    let mut notifications = crate::global::NOTIFICATIONS.lock();
                                    if let Some(n) = notifications.get_mut(notif_id) {
                                        n.take_signals() as u64
                                    } else {
                                        0
                                    }
                                }
                                Err(_) => break 'sys_invoke u64::MAX,
                            }
                        }
                        Err(_) => break 'sys_invoke u64::MAX,
                    }
                } else {
                    u64::MAX
                }
            };
            sys_result
        }
        _ => u64::MAX,
    };

    // Validate the return frame before sysretq executes.
    // A non-canonical RCX would cause #GP(0) at CPL 0 (sysret vulnerability).
    // Forbidden RFLAGS bits in R11 could grant user code IOPL or other
    // dangerous state.
    if !validate_sysret_frame(frame) {
        println!(
            "GAXERA ERROR: SYSRET_VALIDATION_FAILED rcx={:#018x} r11={:#018x} rsp={:#018x}",
            frame.rcx, frame.r11, frame.rsp
        );
        #[cfg(feature = "qemu-test")]
        // SAFETY: Hardware invariant or verified by caller.
        unsafe {
            crate::arch::x86_64::qemu::exit_failure();
        }
        #[cfg(not(feature = "qemu-test"))]
        crate::serial::halt();
    }
    // crate::println!("GAXERA: SYSCALL RET rax={}", frame.rax);
}

fn yield_current_thread() -> Result<(), ()> {
    // SAFETY: Hardware invariant or verified by caller.
    let cpu_local = unsafe { cpu::get_cpu_local() };
    // SAFETY: Hardware invariant or verified by caller.
    let scheduler_cell = unsafe { &mut *cpu_local.scheduler.get() };
    let scheduler = scheduler_cell.as_mut().ok_or(())?;
    let current_id = match scheduler.current_thread() {
        Some(id) => id,
        None => return Err(()),
    };
    let next_id = match scheduler.next_runnable() {
        Some(id) => id,
        None => return Ok(()),
    };

    let result = crate::arch::x86_64::preemption::reschedule(scheduler, current_id, next_id);
    result
}

/// Creates the kernel-owned components of a child process as one transaction.
///
/// The caller has already resolved the current thread and CSpace, but no
/// capability or registry lock is held here.  This is intentional: process
/// creation needs the global lock order `RESOURCE_DOMAINS -> CAPABILITY_SYSTEM
/// -> OBJECT_ARENA -> physical allocator -> typed registries`, while the
/// general syscall path resolves ordinary calls in a shorter scope.
fn create_process_syscall(
    factory_handle: gaxera_abi::Handle,
    parent_cspace_id: kernel_core::object::ObjectId,
    frame: &mut SyscallFrame,
) -> u64 {
    let supervisor_id = match process_for_cspace(parent_cspace_id) {
        Some(id) => id,
        None => return gaxera_abi::status::INVALID_HANDLE,
    };
    let factory_id = {
        let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
        let system = match system_guard.as_mut() {
            Some(system) => system,
            None => return gaxera_abi::status::INTERNAL_ERROR,
        };
        let arena_guard = crate::global::OBJECT_ARENA.lock();
        let arena = match arena_guard.as_ref() {
            Some(arena) => arena,
            None => return gaxera_abi::status::INTERNAL_ERROR,
        };
        let cspaces = crate::global::CAPABILITY_SPACES.lock();
        let cspace = match cspaces.get(parent_cspace_id) {
            Some(cspace) => cspace,
            None => return gaxera_abi::status::INVALID_HANDLE,
        };
        match system.lookup(
            cspace,
            factory_handle,
            gaxera_abi::ObjectType::Factory,
            gaxera_abi::Rights::FACTORY,
            arena,
        ) {
            Ok(id) => id,
            Err(_) => return gaxera_abi::status::RIGHTS_DENIED,
        }
    };

    let factory = {
        let factories = crate::global::FACTORIES.lock();
        match factories.get(factory_id) {
            Some(factory) => *factory,
            None => return gaxera_abi::status::INVALID_HANDLE,
        }
    };

    let max_objects = match u32::try_from(frame.rdx) {
        Ok(value) if value != 0 => value,
        _ => return gaxera_abi::status::INVALID_ARGUMENT,
    };
    let max_capabilities = match u32::try_from(frame.r10) {
        Ok(value) if value != 0 => value,
        _ => return gaxera_abi::status::INVALID_ARGUMENT,
    };
    let max_memory = frame.r8;
    if max_memory == 0 {
        return gaxera_abi::status::INVALID_ARGUMENT;
    }

    let parent_domain_id = factory.domain();
    let child_limits = kernel_core::resource::ResourceLimits {
        objects: max_objects,
        capabilities: max_capabilities,
        memory_bytes: max_memory,
    };

    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
    let arena = match arena_guard.as_mut() {
        Some(arena) => arena,
        None => return gaxera_abi::status::INTERNAL_ERROR,
    };

    let child_domain_id = {
        let parent = match domains.get_mut(parent_domain_id.object_id()) {
            Some(parent) => parent,
            None => return gaxera_abi::status::INVALID_HANDLE,
        };
        match arena.create_resource_domain(parent) {
            Ok(id) => kernel_core::resource::ResourceDomainId::new(id),
            Err(_) => return gaxera_abi::status::RESOURCE_EXHAUSTED,
        }
    };

    let mut child_domain = {
        let parent = match domains.get_mut(parent_domain_id.object_id()) {
            Some(parent) => parent,
            None => return gaxera_abi::status::INVALID_HANDLE,
        };
        match kernel_core::resource::ResourceDomain::new_child(
            child_domain_id,
            parent,
            child_limits,
        ) {
            Ok(domain) => domain,
            Err(_) => {
                let _ = arena.destroy(parent, child_domain_id.object_id());
                return gaxera_abi::status::RESOURCE_EXHAUSTED;
            }
        }
    };

    let process_id = match arena.create_process(&mut child_domain) {
        Ok(id) => id,
        Err(_) => {
            refund_child_domain(&mut domains, &mut child_domain);
            let _ = arena.destroy(
                domains.get_mut(parent_domain_id.object_id()).unwrap(),
                child_domain_id.object_id(),
            );
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    };

    let mut child_aspace = match arena.create_address_space(&mut child_domain) {
        Ok(id) => {
            let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
            let allocator = match physical.as_deref_mut() {
                Some(allocator) => allocator,
                None => {
                    let _ = arena.destroy(&mut child_domain, process_id);
                    refund_child_domain(&mut domains, &mut child_domain);
                    let _ = arena.destroy(
                        domains.get_mut(parent_domain_id.object_id()).unwrap(),
                        child_domain_id.object_id(),
                    );
                    return gaxera_abi::status::RESOURCE_EXHAUSTED;
                }
            };
            match crate::arch::x86_64::address_space::X86AddressSpace::new_dynamic(allocator) {
                Ok(arch) => Some((id, kernel_core::address_space::AddressSpace::new(id, arch))),
                Err(_) => {
                    let _ = arena.destroy(&mut child_domain, process_id);
                    let _ = arena.destroy(&mut child_domain, id);
                    refund_child_domain(&mut domains, &mut child_domain);
                    let _ = arena.destroy(
                        domains.get_mut(parent_domain_id.object_id()).unwrap(),
                        child_domain_id.object_id(),
                    );
                    return gaxera_abi::status::RESOURCE_EXHAUSTED;
                }
            }
        }
        Err(_) => {
            let _ = arena.destroy(&mut child_domain, process_id);
            refund_child_domain(&mut domains, &mut child_domain);
            let _ = arena.destroy(
                domains.get_mut(parent_domain_id.object_id()).unwrap(),
                child_domain_id.object_id(),
            );
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    };

    let cspace_id = match arena.create_capability_space(&mut child_domain) {
        Ok(id) => id,
        Err(_) => {
            let (_, aspace) = child_aspace.take().unwrap();
            let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
            if let Some(allocator) = physical.as_deref_mut() {
                let _ = aspace.arch.destroy(allocator);
            }
            let _ = arena.destroy(&mut child_domain, process_id);
            refund_child_domain(&mut domains, &mut child_domain);
            let _ = arena.destroy(
                domains.get_mut(parent_domain_id.object_id()).unwrap(),
                child_domain_id.object_id(),
            );
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    };
    let mut child_cspace = match kernel_core::capability::CapabilitySpace::try_new(
        &child_domain,
        max_capabilities as usize,
    ) {
        Ok(cspace) => cspace,
        Err(_) => {
            let (_, aspace) = child_aspace.take().unwrap();
            let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
            if let Some(allocator) = physical.as_deref_mut() {
                let _ = aspace.arch.destroy(allocator);
            }
            let _ = arena.destroy(&mut child_domain, cspace_id);
            let _ = arena.destroy(&mut child_domain, process_id);
            refund_child_domain(&mut domains, &mut child_domain);
            let _ = arena.destroy(
                domains.get_mut(parent_domain_id.object_id()).unwrap(),
                child_domain_id.object_id(),
            );
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    };

    let thread_id = match arena.create_thread(&mut child_domain) {
        Ok(id) => id,
        Err(_) => {
            let (_, aspace) = child_aspace.take().unwrap();
            let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
            if let Some(allocator) = physical.as_deref_mut() {
                let _ = aspace.arch.destroy(allocator);
            }
            let _ = arena.destroy(&mut child_domain, cspace_id);
            let _ = arena.destroy(&mut child_domain, process_id);
            refund_child_domain(&mut domains, &mut child_domain);
            let _ = arena.destroy(
                domains.get_mut(parent_domain_id.object_id()).unwrap(),
                child_domain_id.object_id(),
            );
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    };

    let notification_id = match arena.create_notification(&mut child_domain) {
        Ok(id) => id,
        Err(_) => {
            let (_, aspace) = child_aspace.take().unwrap();
            let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
            if let Some(allocator) = physical.as_deref_mut() {
                let _ = aspace.arch.destroy(allocator);
            }
            let _ = arena.destroy(&mut child_domain, thread_id);
            let _ = arena.destroy(&mut child_domain, cspace_id);
            let _ = arena.destroy(&mut child_domain, process_id);
            refund_child_domain(&mut domains, &mut child_domain);
            let _ = arena.destroy(
                domains.get_mut(parent_domain_id.object_id()).unwrap(),
                child_domain_id.object_id(),
            );
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    };

    let mut process = kernel_core::process::Process::new(process_id, child_domain_id);
    if process.bind_supervisor(supervisor_id).is_err() {
        return gaxera_abi::status::INTERNAL_ERROR;
    }
    if process
        .bind_address_space(child_aspace.as_ref().unwrap().0)
        .is_err()
        || process.bind_capability_space(cspace_id).is_err()
        || process.bind_main_thread(thread_id).is_err()
        || process.bind_exit_notification(notification_id).is_err()
    {
        return gaxera_abi::status::INTERNAL_ERROR;
    }

    // SAFETY: Process creation runs on the BSP with the active kernel page
    // tables selected; this mapper is used only for the unpublished stack.
    let mut active_pt = unsafe { crate::arch::x86_64::paging::KernelPageTables::active() };
    let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
    let allocator = match physical.as_deref_mut() {
        Some(allocator) => allocator,
        None => {
            drop(physical);
            return rollback_process_components(
                &mut domains,
                arena,
                &mut child_domain,
                parent_domain_id,
                child_domain_id,
                process_id,
                child_aspace.as_ref().unwrap().0,
                cspace_id,
                thread_id,
                notification_id,
                &mut child_aspace,
            );
        }
    };

    let stack = match crate::arch::x86_64::stack::KernelStack::allocate(&mut active_pt, allocator) {
        Ok(stack) => stack,
        Err(_) => {
            drop(physical);
            return rollback_process_components(
                &mut domains,
                arena,
                &mut child_domain,
                parent_domain_id,
                child_domain_id,
                process_id,
                child_aspace.as_ref().unwrap().0,
                cspace_id,
                thread_id,
                notification_id,
                &mut child_aspace,
            );
        }
    };
    drop(physical);

    // Insert the process capability only after every fallible child component
    // and the kernel stack have succeeded. This keeps the rollback point
    // before any user-visible authority is published.
    drop(arena_guard);
    let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
    let system = match system_guard.as_mut() {
        Some(system) => system,
        None => {
            drop(system_guard);
            return rollback_process_with_stack(
                stack,
                &mut active_pt,
                &mut domains,
                &mut child_domain,
                parent_domain_id,
                child_domain_id,
                process_id,
                child_aspace.as_ref().unwrap().0,
                cspace_id,
                thread_id,
                notification_id,
                &mut child_aspace,
            );
        }
    };
    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
    let arena = match arena_guard.as_mut() {
        Some(arena) => arena,
        None => {
            drop(arena_guard);
            drop(system_guard);
            return rollback_process_with_stack(
                stack,
                &mut active_pt,
                &mut domains,
                &mut child_domain,
                parent_domain_id,
                child_domain_id,
                process_id,
                child_aspace.as_ref().unwrap().0,
                cspace_id,
                thread_id,
                notification_id,
                &mut child_aspace,
            );
        }
    };
    let child_aspace_id = child_aspace.as_ref().unwrap().0;
    let bootstrap = match provision_child_bootstrap(
        system,
        arena,
        &mut child_domain,
        &mut child_cspace,
        &mut child_aspace.as_mut().unwrap().1,
        process_id,
        child_aspace_id,
        cspace_id,
        thread_id,
        notification_id,
    ) {
        Ok(bootstrap) => bootstrap,
        Err(_status) => {
            drop(arena_guard);
            drop(system_guard);
            return rollback_process_with_stack(
                stack,
                &mut active_pt,
                &mut domains,
                &mut child_domain,
                parent_domain_id,
                child_domain_id,
                process_id,
                child_aspace.as_ref().unwrap().0,
                cspace_id,
                thread_id,
                notification_id,
                &mut child_aspace,
            );
        }
    };
    if process
        .bind_bootstrap_manifest(
            bootstrap.manifest_frame.start_address().as_u64(),
            gaxera_abi::boot::BOOTSTRAP_MANIFEST_VADDR,
            u32::from(gaxera_abi::boot::BootstrapManifest::HEADER_SIZE)
                + u32::from(gaxera_abi::boot::BootstrapManifest::ENTRY_SIZE) * 5,
        )
        .is_err()
        || process
            .bind_bootstrap_factory(bootstrap.factory_id)
            .is_err()
        || process.prepare().is_err()
    {
        return rollback_process_bootstrap(
            stack,
            &mut active_pt,
            &mut domains,
            arena,
            &mut child_domain,
            parent_domain_id,
            child_domain_id,
            process_id,
            child_aspace.as_ref().unwrap().0,
            cspace_id,
            thread_id,
            notification_id,
            &mut child_aspace,
            system,
            &mut child_cspace,
            &bootstrap.handles,
            Some(bootstrap.factory_id),
            Some(bootstrap.manifest_frame),
        );
    }
    let mut cspaces_guard = crate::global::CAPABILITY_SPACES.lock();
    let parent_cspace = match cspaces_guard.get_mut(parent_cspace_id) {
        Some(cspace) => cspace,
        None => {
            drop(cspaces_guard);
            return rollback_process_bootstrap(
                stack,
                &mut active_pt,
                &mut domains,
                arena,
                &mut child_domain,
                parent_domain_id,
                child_domain_id,
                process_id,
                child_aspace.as_ref().unwrap().0,
                cspace_id,
                thread_id,
                notification_id,
                &mut child_aspace,
                system,
                &mut child_cspace,
                &bootstrap.handles,
                Some(bootstrap.factory_id),
                Some(bootstrap.manifest_frame),
            );
        }
    };
    let parent_domain = match domains.get_mut(parent_domain_id.object_id()) {
        Some(domain) => domain,
        None => {
            drop(cspaces_guard);
            return rollback_process_bootstrap(
                stack,
                &mut active_pt,
                &mut domains,
                arena,
                &mut child_domain,
                parent_domain_id,
                child_domain_id,
                process_id,
                child_aspace.as_ref().unwrap().0,
                cspace_id,
                thread_id,
                notification_id,
                &mut child_aspace,
                system,
                &mut child_cspace,
                &bootstrap.handles,
                Some(bootstrap.factory_id),
                Some(bootstrap.manifest_frame),
            );
        }
    };
    let process_handle = match system.insert_root(
        parent_cspace,
        parent_domain,
        process_id,
        gaxera_abi::ObjectType::Process,
        gaxera_abi::Rights::MANAGE,
        arena,
    ) {
        Ok(handle) => handle,
        Err(_) => {
            drop(cspaces_guard);
            return rollback_process_bootstrap(
                stack,
                &mut active_pt,
                &mut domains,
                arena,
                &mut child_domain,
                parent_domain_id,
                child_domain_id,
                process_id,
                child_aspace.as_ref().unwrap().0,
                cspace_id,
                thread_id,
                notification_id,
                &mut child_aspace,
                system,
                &mut child_cspace,
                &bootstrap.handles,
                Some(bootstrap.factory_id),
                Some(bootstrap.manifest_frame),
            );
        }
    };
    drop(cspaces_guard);
    drop(arena_guard);
    drop(system_guard);

    let arch_thread = crate::arch::x86_64::thread::ArchThread {
        stack,
        context: crate::arch::x86_64::context::Context::empty(),
        cr3: child_aspace.as_ref().map(|(_, aspace)| {
            x86_64::structures::paging::PhysFrame::containing_address(x86_64::PhysAddr::new(
                aspace.arch.root_token(),
            ))
        }),
    };
    let mut thread = kernel_core::thread::Thread::new(
        thread_id,
        child_aspace.as_ref().map(|(id, _)| *id),
        arch_thread,
    );
    thread.set_cspace(cspace_id);

    let (_, aspace) = child_aspace.take().unwrap();
    crate::global::ADDRESS_SPACES
        .lock()
        .insert(aspace.id(), aspace);
    crate::global::CAPABILITY_SPACES
        .lock()
        .insert(cspace_id, child_cspace);
    crate::global::FACTORIES
        .lock()
        .insert(bootstrap.factory_id, bootstrap.factory);
    // SAFETY: The new thread is unpublished and process creation runs on the
    // single BSP, so the architecture-owned table has exclusive access.
    unsafe { crate::arch::x86_64::thread::THREADS.insert(thread) };
    crate::global::NOTIFICATIONS.lock().insert(
        notification_id,
        kernel_core::notification::Notification::new(notification_id),
    );
    crate::global::PROCESSES.lock().insert(process_id, process);
    child_domain.add_process_ref();
    domains.insert(child_domain_id.object_id(), child_domain);
    frame.rdx = process_handle.raw();
    gaxera_abi::status::SUCCESS
}

#[allow(clippy::too_many_arguments)]
fn process_state_code(state: kernel_core::process::ProcessState) -> u64 {
    match state {
        kernel_core::process::ProcessState::New => 0,
        kernel_core::process::ProcessState::Prepared => 1,
        kernel_core::process::ProcessState::Runnable => 2,
        kernel_core::process::ProcessState::Running => 3,
        kernel_core::process::ProcessState::ExitRequested => 4,
        kernel_core::process::ProcessState::Exiting => 5,
        kernel_core::process::ProcessState::Zombie => 6,
        kernel_core::process::ProcessState::Reaped => 7,
    }
}

fn delete_bootstrap_handles(
    system: &mut kernel_core::capability::CapabilitySystem,
    cspace: &mut kernel_core::capability::CapabilitySpace,
    domain: &mut kernel_core::resource::ResourceDomain,
    handles: &[gaxera_abi::Handle],
) {
    for &handle in handles.iter().rev() {
        if handle.is_valid() {
            let _ = system.delete(cspace, domain, handle);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rollback_process_bootstrap(
    stack: crate::arch::x86_64::stack::KernelStack,
    active_pt: &mut crate::arch::x86_64::paging::KernelPageTables,
    domains: &mut kernel_core::registry::BTreeRegistry<kernel_core::resource::ResourceDomain>,
    arena: &mut kernel_core::object::ObjectArena,
    child_domain: &mut kernel_core::resource::ResourceDomain,
    parent_domain_id: kernel_core::resource::ResourceDomainId,
    child_domain_id: kernel_core::resource::ResourceDomainId,
    process_id: kernel_core::object::ObjectId,
    aspace_id: kernel_core::object::ObjectId,
    cspace_id: kernel_core::object::ObjectId,
    thread_id: kernel_core::object::ObjectId,
    notification_id: kernel_core::object::ObjectId,
    child_aspace: &mut Option<(
        kernel_core::object::ObjectId,
        kernel_core::address_space::AddressSpace<
            crate::arch::x86_64::address_space::X86AddressSpace,
        >,
    )>,
    system: &mut kernel_core::capability::CapabilitySystem,
    child_cspace: &mut kernel_core::capability::CapabilitySpace,
    child_handles: &[gaxera_abi::Handle],
    child_factory_id: Option<kernel_core::object::ObjectId>,
    manifest_frame: Option<x86_64::structures::paging::PhysFrame>,
) -> u64 {
    delete_bootstrap_handles(system, child_cspace, child_domain, child_handles);
    if let Some(factory_id) = child_factory_id {
        let _ = arena.destroy(child_domain, factory_id);
    }

    if let Some((_, aspace)) = child_aspace.as_mut() {
        if manifest_frame.is_some() {
            let _ = aspace
                .arch
                .unmap_range(gaxera_abi::boot::BOOTSTRAP_MANIFEST_VADDR, 1);
        }
    }
    if let Some(frame) = manifest_frame {
        let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = physical.as_deref_mut() {
            // SAFETY: The manifest frame is owned by this failed creation
            // transaction and has just been unmapped, if it was mapped.
            unsafe { allocator.deallocate_frame(frame) };
        }
        let _ = child_domain.release_memory(crate::memory::physical::PAGE_SIZE);
    }
    {
        let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = physical.as_deref_mut() {
            stack.reclaim(active_pt, allocator);
        }
    }
    rollback_process_components(
        domains,
        arena,
        child_domain,
        parent_domain_id,
        child_domain_id,
        process_id,
        aspace_id,
        cspace_id,
        thread_id,
        notification_id,
        child_aspace,
    )
}

struct ChildBootstrap {
    factory_id: kernel_core::object::ObjectId,
    factory: kernel_core::object::Factory,
    handles: [gaxera_abi::Handle; 5],
    manifest_frame: x86_64::structures::paging::PhysFrame,
}

#[allow(clippy::too_many_arguments)]
fn provision_child_bootstrap(
    system: &mut kernel_core::capability::CapabilitySystem,
    arena: &mut kernel_core::object::ObjectArena,
    child_domain: &mut kernel_core::resource::ResourceDomain,
    child_cspace: &mut kernel_core::capability::CapabilitySpace,
    child_aspace: &mut kernel_core::address_space::AddressSpace<
        crate::arch::x86_64::address_space::X86AddressSpace,
    >,
    process_id: kernel_core::object::ObjectId,
    aspace_id: kernel_core::object::ObjectId,
    cspace_id: kernel_core::object::ObjectId,
    thread_id: kernel_core::object::ObjectId,
    notification_id: kernel_core::object::ObjectId,
) -> Result<ChildBootstrap, u64> {
    let factory = kernel_core::object::Factory::new_root(
        child_domain,
        gaxera_abi::ObjectTypeSet::of(gaxera_abi::ObjectType::MemoryObject),
    );
    let factory_id = arena
        .create(
            child_domain,
            kernel_core::object::Factory::new_root(
                child_domain,
                gaxera_abi::ObjectTypeSet::of(gaxera_abi::ObjectType::Factory),
            ),
            gaxera_abi::ObjectType::Factory,
        )
        .map_err(|_| gaxera_abi::status::RESOURCE_EXHAUSTED)?;
    let mut handles = [gaxera_abi::Handle::INVALID; 5];
    let mut count = 0usize;

    macro_rules! insert_root {
        ($object:expr, $object_type:expr, $rights:expr) => {{
            let result = system.insert_root(
                child_cspace,
                child_domain,
                $object,
                $object_type,
                $rights,
                arena,
            );
            match result {
                Ok(handle) => {
                    handles[count] = handle;
                    count += 1;
                    handle
                }
                Err(_) => {
                    delete_bootstrap_handles(system, child_cspace, child_domain, &handles[..count]);
                    let _ = arena.destroy(child_domain, factory_id);
                    return Err(gaxera_abi::status::RESOURCE_EXHAUSTED);
                }
            }
        }};
    }

    let h_aspace = insert_root!(
        aspace_id,
        gaxera_abi::ObjectType::AddressSpace,
        gaxera_abi::Rights::MAP
    );
    let h_cspace = insert_root!(
        cspace_id,
        gaxera_abi::ObjectType::CapabilitySpace,
        gaxera_abi::Rights::MANAGE
    );
    let h_thread = insert_root!(
        thread_id,
        gaxera_abi::ObjectType::Thread,
        gaxera_abi::Rights::MANAGE
    );
    let h_factory = insert_root!(
        factory_id,
        gaxera_abi::ObjectType::Factory,
        gaxera_abi::Rights::FACTORY
    );
    let h_exit = insert_root!(
        notification_id,
        gaxera_abi::ObjectType::Notification,
        gaxera_abi::Rights::WAIT
    );
    let manifest =
        child_bootstrap_manifest(process_id, h_aspace, h_cspace, h_thread, h_factory, h_exit);

    if child_domain
        .charge_memory(crate::memory::physical::PAGE_SIZE)
        .is_err()
    {
        delete_bootstrap_handles(system, child_cspace, child_domain, &handles[..count]);
        let _ = arena.destroy(child_domain, factory_id);
        return Err(gaxera_abi::status::RESOURCE_EXHAUSTED);
    }
    let manifest_frame = {
        let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
        match physical
            .as_deref_mut()
            .and_then(|allocator| allocator.allocate_frame())
        {
            Some(frame) => frame,
            None => {
                let _ = child_domain.release_memory(crate::memory::physical::PAGE_SIZE);
                delete_bootstrap_handles(system, child_cspace, child_domain, &handles[..count]);
                let _ = arena.destroy(child_domain, factory_id);
                return Err(gaxera_abi::status::RESOURCE_EXHAUSTED);
            }
        }
    };
    let manifest_phys = manifest_frame.start_address().as_u64();
    if child_aspace
        .arch
        .map_frames(
            gaxera_abi::boot::BOOTSTRAP_MANIFEST_VADDR,
            &[manifest_phys],
            gaxera_abi::Rights::READ,
        )
        .is_err()
    {
        let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = physical.as_deref_mut() {
            // SAFETY: Mapping failed before ownership escaped this transaction.
            unsafe { allocator.deallocate_frame(manifest_frame) };
        }
        let _ = child_domain.release_memory(crate::memory::physical::PAGE_SIZE);
        delete_bootstrap_handles(system, child_cspace, child_domain, &handles[..count]);
        let _ = arena.destroy(child_domain, factory_id);
        return Err(gaxera_abi::status::RESOURCE_EXHAUSTED);
    }
    let manifest_hhdm = crate::memory::mapping::HHDM_BASE + manifest_phys;
    // SAFETY: The page is mapped read-only/NX in the unpublished child and
    // the HHDM alias is the kernel's only write route.
    unsafe {
        core::ptr::write_bytes(
            manifest_hhdm as *mut u8,
            0,
            crate::memory::physical::PAGE_SIZE as usize,
        );
        core::ptr::write(
            manifest_hhdm as *mut gaxera_abi::boot::BootstrapManifest,
            manifest,
        );
    }
    Ok(ChildBootstrap {
        factory_id,
        factory,
        handles,
        manifest_frame,
    })
}

fn child_bootstrap_manifest(
    process_id: kernel_core::object::ObjectId,
    aspace: gaxera_abi::Handle,
    cspace: gaxera_abi::Handle,
    thread: gaxera_abi::Handle,
    factory: gaxera_abi::Handle,
    exit_notification: gaxera_abi::Handle,
) -> gaxera_abi::boot::BootstrapManifest {
    let mut entries = [gaxera_abi::boot::BootstrapCapability {
        role: 0,
        object_type: 0,
        flags: 0,
        rights: 0,
        handle: gaxera_abi::Handle::INVALID,
        metadata: 0,
    }; gaxera_abi::boot::MAX_BOOTSTRAP_CAPABILITIES];
    let values = [
        (
            gaxera_abi::boot::BootstrapRole::SelfAddressSpace,
            gaxera_abi::ObjectType::AddressSpace,
            gaxera_abi::Rights::MAP,
            aspace,
        ),
        (
            gaxera_abi::boot::BootstrapRole::SelfCapabilitySpace,
            gaxera_abi::ObjectType::CapabilitySpace,
            gaxera_abi::Rights::MANAGE,
            cspace,
        ),
        (
            gaxera_abi::boot::BootstrapRole::SelfThread,
            gaxera_abi::ObjectType::Thread,
            gaxera_abi::Rights::MANAGE,
            thread,
        ),
        (
            gaxera_abi::boot::BootstrapRole::HeapFactory,
            gaxera_abi::ObjectType::Factory,
            gaxera_abi::Rights::FACTORY,
            factory,
        ),
        (
            gaxera_abi::boot::BootstrapRole::ExitNotification,
            gaxera_abi::ObjectType::Notification,
            gaxera_abi::Rights::WAIT,
            exit_notification,
        ),
    ];
    for (index, (role, object_type, rights, handle)) in values.into_iter().enumerate() {
        entries[index] = gaxera_abi::boot::BootstrapCapability {
            role: role as u16,
            object_type: object_type as u8,
            flags: 0,
            rights: rights.bits(),
            handle,
            metadata: 0,
        };
    }
    let count = values.len() as u16;
    gaxera_abi::boot::BootstrapManifest {
        magic: gaxera_abi::boot::BootstrapManifest::MAGIC,
        abi_version: gaxera_abi::boot::BootstrapManifest::ABI_VERSION,
        header_size: gaxera_abi::boot::BootstrapManifest::HEADER_SIZE,
        entry_size: gaxera_abi::boot::BootstrapManifest::ENTRY_SIZE,
        total_size: u32::from(gaxera_abi::boot::BootstrapManifest::HEADER_SIZE)
            + u32::from(gaxera_abi::boot::BootstrapManifest::ENTRY_SIZE) * u32::from(count),
        entry_count: count,
        reserved: 0,
        process_token: process_id.raw(),
        parent_token: 0,
        entries,
    }
}

/// Prepare a newly-created thread for a later scheduler start without making
/// it runnable. The process state machine owns the publication point; this
/// helper only builds the architecture context after validating all handles.
fn configure_process_thread(
    thread_id: kernel_core::object::ObjectId,
    address_space_id: kernel_core::object::ObjectId,
    cspace_id: kernel_core::object::ObjectId,
    rip: u64,
    rsp: u64,
    bootstrap_manifest: Option<(u64, u64, u32)>,
) -> Result<(), u64> {
    if !is_user_return_address(rip) || !is_user_return_address(rsp) || !rsp.is_multiple_of(16) {
        return Err(gaxera_abi::status::INVALID_ARGUMENT);
    }

    let aspaces = crate::global::ADDRESS_SPACES.lock();
    let aspace = aspaces
        .get(address_space_id)
        .ok_or(gaxera_abi::status::INVALID_HANDLE)?;
    let cr3 = aspace.arch.root_token();
    drop(aspaces);

    // SAFETY: The thread is unpublished or exclusively controlled by its
    // owning Process during ConfigureMainThread.
    let thread = unsafe {
        crate::arch::x86_64::thread::THREADS
            .get_mut(thread_id)
            .ok_or(gaxera_abi::status::INVALID_HANDLE)?
    };
    if thread.state() != kernel_core::thread::ThreadState::New {
        return Err(gaxera_abi::status::INVALID_ARGUMENT);
    }
    thread.set_cspace(cspace_id);

    let stack_top = thread.arch.stack.top().as_mut_ptr::<u8>();
    // SAFETY: KernelStack owns a 16-page mapped region; the context frame is
    // placed at its top and remains valid until the thread is reaped.
    unsafe {
        let frame_ptr = stack_top.sub(core::mem::size_of::<SyscallFrame>()) as *mut SyscallFrame;
        core::ptr::write_bytes(frame_ptr, 0, 1);
        (*frame_ptr).rcx = rip;
        (*frame_ptr).rsp = rsp;
        (*frame_ptr).r11 = 0x202;
        let (_, manifest_vaddr, manifest_size) =
            bootstrap_manifest.ok_or(gaxera_abi::status::INVALID_ARGUMENT)?;
        (*frame_ptr).rdi = manifest_vaddr;
        (*frame_ptr).rsi = u64::from(manifest_size);

        let ret_addr_ptr = (frame_ptr as *mut u64).sub(1);
        *ret_addr_ptr = syscall_return as *const () as usize as u64;
        let context_regs_ptr = ret_addr_ptr.sub(6);
        core::ptr::write_bytes(context_regs_ptr, 0, 6);
        thread.arch.context = crate::arch::x86_64::context::Context {
            rsp: context_regs_ptr as usize as u64,
        };
        thread.arch.cr3 = Some(
            x86_64::structures::paging::PhysFrame::from_start_address(x86_64::PhysAddr::new(cr3))
                .map_err(|_| gaxera_abi::status::INVALID_ARGUMENT)?,
        );
    }
    Ok(())
}

fn process_for_thread(
    thread_id: kernel_core::object::ObjectId,
) -> Option<kernel_core::object::ObjectId> {
    let mut processes = crate::global::PROCESSES.lock();
    processes
        .iter_mut()
        .find(|(_, process)| process.main_thread() == Some(thread_id))
        .map(|(id, _)| id)
}

fn process_for_cspace(
    cspace_id: kernel_core::object::ObjectId,
) -> Option<kernel_core::object::ObjectId> {
    let mut processes = crate::global::PROCESSES.lock();
    processes
        .iter_mut()
        .find(|(_, process)| process.capability_space() == Some(cspace_id))
        .map(|(id, _)| id)
}

/// Converts the currently executing child into a Zombie and switches away
/// from its dying thread.  No child-owned capability or address-space object
/// is destroyed on this stack: the supervisor's Reap operation performs that
/// work after the thread is no longer executing.
fn exit_current_process(
    process_id: kernel_core::object::ObjectId,
    current_thread_id: kernel_core::object::ObjectId,
    status: u64,
) -> ! {
    let (exit_notification, supervisor_thread) = {
        let mut processes = crate::global::PROCESSES.lock();
        let process = processes.get_mut(process_id).unwrap();
        if process.request_exit(status).is_err()
            || process.mark_exiting().is_err()
            || process.mark_zombie().is_err()
        {
            crate::serial::halt()
        }
        let notification = process.exit_notification();
        let supervisor_thread = process
            .supervisor()
            .and_then(|supervisor_id| processes.get(supervisor_id))
            .and_then(|supervisor| supervisor.main_thread());
        (notification, supervisor_thread)
    };

    if let Some(notification_id) = exit_notification {
        let mut notifications = crate::global::NOTIFICATIONS.lock();
        if let Some(notification) = notifications.get_mut(notification_id) {
            notification.signal(1);
        }
    }

    // Marking the thread Dying before removing it from the scheduler makes a
    // later stale Start attempt fail the Thread state machine.
    // SAFETY: This is the current BSP-owned thread and interrupts are disabled
    // during syscall dispatch.
    unsafe {
        if let Some(thread) = crate::arch::x86_64::thread::THREADS.get_mut(current_thread_id) {
            let _ = thread.make_dying();
        }
    }

    // SAFETY: The syscall runs on the BSP with exclusive scheduler access.
    let cpu_local = unsafe { cpu::get_cpu_local() };
    let scheduler = unsafe { &mut *cpu_local.scheduler.get() };
    let scheduler = match scheduler.as_mut() {
        Some(scheduler) => scheduler,
        None => crate::serial::halt(),
    };
    let _ = scheduler.remove_thread(current_thread_id);
    let next = scheduler.dequeue_next();
    match next {
        Some(next_id) => {
            scheduler.set_current_thread(Some(next_id));
            // SAFETY: `current_thread_id` is dying and no longer queued;
            // `next_id` was dequeued from the runnable queue.
            let _ = crate::arch::x86_64::preemption::switch_to_next(current_thread_id, next_id);
        }
        None => {
            if let Some(supervisor_id) = supervisor_thread {
                scheduler.set_current_thread(Some(supervisor_id));
                let _ = crate::arch::x86_64::preemption::switch_to_next(
                    current_thread_id,
                    supervisor_id,
                );
            } else {
                #[cfg(feature = "qemu-test")]
                // SAFETY: Only the root test process can reach this branch;
                // child processes return to their recorded supervisor above.
                unsafe {
                    if status == 0 {
                        crate::arch::x86_64::qemu::exit_success()
                    } else {
                        crate::arch::x86_64::qemu::exit_failure()
                    }
                };
                #[cfg(not(feature = "qemu-test"))]
                {
                    let Some(idle_id) = crate::arch::x86_64::thread::idle_thread() else {
                        crate::serial::halt()
                    };
                    scheduler.set_current_thread(Some(idle_id));
                    let _ =
                        crate::arch::x86_64::preemption::switch_to_next(current_thread_id, idle_id);
                }
            }
        }
    }
    crate::serial::halt()
}

fn reap_process_syscall(
    process_id: kernel_core::object::ObjectId,
    caller_cspace_id: kernel_core::object::ObjectId,
    process_handle: gaxera_abi::Handle,
) -> u64 {
    let (state, domain_id, aspace_id, cspace_id, thread_id, notification_id, manifest, factory_id) = {
        let processes = crate::global::PROCESSES.lock();
        let process = match processes.get(process_id) {
            Some(process) => process,
            None => return gaxera_abi::status::INVALID_HANDLE,
        };
        (
            process.state(),
            process.domain(),
            process.address_space(),
            process.capability_space(),
            process.main_thread(),
            process.exit_notification(),
            process.bootstrap_manifest(),
            process.bootstrap_factory(),
        )
    };
    if state != kernel_core::process::ProcessState::Zombie {
        return gaxera_abi::status::INVALID_ARGUMENT;
    }
    let (aspace_id, cspace_id, thread_id, notification_id, factory_id, manifest) = match (
        aspace_id,
        cspace_id,
        thread_id,
        notification_id,
        factory_id,
        manifest,
    ) {
        (Some(a), Some(c), Some(t), Some(n), Some(f), Some(m)) => (a, c, t, n, f, m),
        _ => return gaxera_abi::status::INVALID_HANDLE,
    };

    let handles = {
        let cspaces = crate::global::CAPABILITY_SPACES.lock();
        match cspaces
            .get(cspace_id)
            .and_then(|cspace| cspace.snapshot_handles().ok())
        {
            Some(handles) => handles,
            None => return gaxera_abi::status::RESOURCE_EXHAUSTED,
        }
    };
    for handle in handles {
        if crate::arch::x86_64::teardown::delete_handle_internal(cspace_id, handle).is_err() {
            return gaxera_abi::status::INTERNAL_ERROR;
        }
    }

    // The process capability is consumed by Reap, after all child-owned
    // handles have released mappings and MemoryObjects.
    if crate::arch::x86_64::teardown::delete_handle_internal(caller_cspace_id, process_handle)
        .is_err()
    {
        return gaxera_abi::status::INTERNAL_ERROR;
    }

    // Remove the immutable bootstrap page and return its charged frame.
    let removed_aspace = {
        let mut aspaces = crate::global::ADDRESS_SPACES.lock();
        aspaces.remove(aspace_id)
    };
    if let Some(aspace) = removed_aspace {
        let mut arch = aspace.arch;
        let _ = arch.unmap_range(manifest.1, 1);
        let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = physical.as_deref_mut() {
            // SAFETY: The process is Zombie and no CPU can execute in this
            // address space; the manifest mapping is exclusively owned here.
            unsafe {
                allocator.deallocate_frame(
                    x86_64::structures::paging::PhysFrame::containing_address(
                        x86_64::PhysAddr::new(manifest.0),
                    ),
                )
            };
            let _ = arch.destroy(allocator);
        }
    }

    let removed_thread = unsafe { crate::arch::x86_64::thread::THREADS.remove(thread_id) };
    if let Some(thread) = removed_thread {
        let mut active = unsafe { crate::arch::x86_64::paging::KernelPageTables::active() };
        let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = physical.as_deref_mut() {
            thread.arch.stack.reclaim(&mut active, allocator);
        }
    }
    let _ = crate::global::CAPABILITY_SPACES.lock().remove(cspace_id);
    let _ = crate::global::NOTIFICATIONS.lock().remove(notification_id);
    let _ = crate::global::FACTORIES.lock().remove(factory_id);
    {
        let mut processes = crate::global::PROCESSES.lock();
        if let Some(process) = processes.get_mut(process_id) {
            let _ = process.reap();
        }
    }
    let _ = crate::global::PROCESSES.lock().remove(process_id);

    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
    let mut child_domain = match domains.remove(domain_id.object_id()) {
        Some(domain) => domain,
        None => return gaxera_abi::status::INVALID_HANDLE,
    };
    let _ = child_domain.release_process_ref();
    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
    let arena = match arena_guard.as_mut() {
        Some(arena) => arena,
        None => return gaxera_abi::status::INTERNAL_ERROR,
    };
    for object_id in [
        process_id,
        aspace_id,
        cspace_id,
        thread_id,
        notification_id,
        factory_id,
    ] {
        let _ = arena.destroy(&mut child_domain, object_id);
    }
    let _ = child_domain.release_memory(crate::memory::physical::PAGE_SIZE);
    let eligible = child_domain.is_eligible_for_destruction();
    if eligible {
        if let Some(parent_id) = child_domain.parent() {
            if let Some(parent) = domains.get_mut(parent_id.object_id()) {
                if arena.destroy(parent, domain_id.object_id()).is_ok()
                    && child_domain.refund_to_parent(parent).is_ok()
                {
                    return gaxera_abi::status::SUCCESS;
                }
            }
        }
    }
    // Delegated resources may intentionally keep the ResourceDomain alive
    // after its process has been reaped.
    domains.insert(domain_id.object_id(), child_domain);
    gaxera_abi::status::SUCCESS
}

#[allow(clippy::too_many_arguments)]
fn install_process_capability(
    process_id: kernel_core::object::ObjectId,
    caller_cspace_id: kernel_core::object::ObjectId,
    target_cspace_id: kernel_core::object::ObjectId,
    target_domain_id: kernel_core::resource::ResourceDomainId,
    source_handle: gaxera_abi::Handle,
    requested_rights: gaxera_abi::Rights,
    role: u64,
    frame: &mut SyscallFrame,
) -> u64 {
    let role = match u16::try_from(role)
        .ok()
        .and_then(|role| gaxera_abi::boot::BootstrapRole::try_from(role).ok())
    {
        Some(role) => role as u16,
        None => return gaxera_abi::status::INVALID_ARGUMENT,
    };
    if caller_cspace_id == target_cspace_id {
        return gaxera_abi::status::INVALID_ARGUMENT;
    }
    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
    let target_domain = match domains.get_mut(target_domain_id.object_id()) {
        Some(domain) => domain,
        None => return gaxera_abi::status::INVALID_HANDLE,
    };
    let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
    let system = match system_guard.as_mut() {
        Some(system) => system,
        None => return gaxera_abi::status::INTERNAL_ERROR,
    };
    let arena_guard = crate::global::OBJECT_ARENA.lock();
    let arena = match arena_guard.as_ref() {
        Some(arena) => arena,
        None => return gaxera_abi::status::INTERNAL_ERROR,
    };
    let mut cspaces = crate::global::CAPABILITY_SPACES.lock();
    let source = match cspaces.get(caller_cspace_id) {
        Some(source) => source as *const kernel_core::capability::CapabilitySpace,
        None => return gaxera_abi::status::INVALID_HANDLE,
    };
    let target = match cspaces.get_mut(target_cspace_id) {
        Some(target) => target as *mut kernel_core::capability::CapabilitySpace,
        None => return gaxera_abi::status::INVALID_HANDLE,
    };
    let source_info = match unsafe { system.inspect(&*source, source_handle, arena) } {
        Ok(info) => info,
        Err(_) => return gaxera_abi::status::INVALID_HANDLE,
    };
    // SAFETY: caller and target CSpace IDs were checked distinct and the
    // registry lock prevents mutation for the duration of the derive.
    let handle = match unsafe {
        system.derive(
            &*source,
            source_handle,
            &mut *target,
            target_domain,
            requested_rights,
            arena,
        )
    } {
        Ok(handle) => handle,
        Err(kernel_core::capability::CapabilityError::RightsEscalation)
        | Err(kernel_core::capability::CapabilityError::RightsDenied) => {
            return gaxera_abi::status::RIGHTS_DENIED;
        }
        Err(kernel_core::capability::CapabilityError::SpaceFull)
        | Err(kernel_core::capability::CapabilityError::NodeCapacity)
        | Err(kernel_core::capability::CapabilityError::Resource(_)) => {
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
        Err(_) => return gaxera_abi::status::INVALID_HANDLE,
    };
    drop(cspaces);
    drop(arena_guard);
    drop(system_guard);
    drop(domains);
    if source_info.object_type == gaxera_abi::ObjectType::MemoryObject {
        let mut memory_objects = crate::global::MEMORY_OBJECTS.lock();
        let incremented = memory_objects
            .get_mut(source_info.object)
            .is_some_and(|memory| memory.inc_capability_ref().is_ok());
        if !incremented {
            let _ = crate::arch::x86_64::teardown::delete_handle_internal(target_cspace_id, handle);
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    } else if source_info.object_type == gaxera_abi::ObjectType::Mapping {
        let mut mappings = crate::global::MAPPINGS.lock();
        let incremented = mappings
            .get_mut(source_info.object)
            .is_some_and(|mapping| mapping.inc_capability_ref().is_ok());
        if !incremented {
            let _ = crate::arch::x86_64::teardown::delete_handle_internal(target_cspace_id, handle);
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    } else if source_info.object_type == gaxera_abi::ObjectType::InterruptObject {
        let mut interrupts = crate::global::INTERRUPTS.lock();
        let incremented = interrupts
            .get_mut(source_info.object)
            .is_some_and(|interrupt| interrupt.inc_capability_ref().is_ok());
        if !incremented {
            let _ = crate::arch::x86_64::teardown::delete_handle_internal(target_cspace_id, handle);
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    } else if source_info.object_type == gaxera_abi::ObjectType::Notification {
        let mut notifications = crate::global::NOTIFICATIONS.lock();
        let incremented = notifications
            .get_mut(source_info.object)
            .is_some_and(|notification| notification.inc_capability_ref().is_ok());
        if !incremented {
            let _ = crate::arch::x86_64::teardown::delete_handle_internal(target_cspace_id, handle);
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    }
    {
        let mut processes = crate::global::PROCESSES.lock();
        let process = match processes.get_mut(process_id) {
            Some(process) => process,
            None => return gaxera_abi::status::INVALID_HANDLE,
        };
        if process.record_installed_role(role).is_err() {
            // A duplicate role is rejected after the capability insertion. The
            // target handle is still revoked transactionally before returning.
            let _ = crate::arch::x86_64::teardown::delete_handle_internal(target_cspace_id, handle);
            return gaxera_abi::status::INVALID_ARGUMENT;
        }
    }
    if append_process_bootstrap_capability(process_id, role, handle, source_info).is_err() {
        let _ = crate::arch::x86_64::teardown::delete_handle_internal(target_cspace_id, handle);
        return gaxera_abi::status::INVALID_ARGUMENT;
    }
    frame.rdx = handle.raw();
    gaxera_abi::status::SUCCESS
}

/// Add an explicitly installed capability to the child's immutable-at-entry
/// manifest.  The manifest page is kernel-owned memory mapped read-only in
/// the child; this HHDM write is the only mutation point before the child is
/// started.
fn append_process_bootstrap_capability(
    process_id: kernel_core::object::ObjectId,
    role: u16,
    handle: gaxera_abi::Handle,
    info: kernel_core::capability::CapabilityInfo,
) -> Result<(), ()> {
    let mut processes = crate::global::PROCESSES.lock();
    let process = processes.get_mut(process_id).ok_or(())?;
    let (manifest_frame, _, _) = process.bootstrap_manifest().ok_or(())?;
    let manifest_hhdm = crate::memory::mapping::HHDM_BASE + manifest_frame;
    // SAFETY: The process manifest frame is allocated and mapped by the
    // process bootstrap transaction.  The process registry lock prevents a
    // concurrent teardown while this kernel-only HHDM update is performed.
    let manifest = unsafe { &mut *(manifest_hhdm as *mut gaxera_abi::boot::BootstrapManifest) };
    let count = usize::from(manifest.entry_count);
    let repeatable = matches!(role, 5 | 8 | 9);
    if count >= gaxera_abi::boot::MAX_BOOTSTRAP_CAPABILITIES
        || (!repeatable
            && manifest.entries[..count]
                .iter()
                .any(|entry| entry.role == role))
    {
        return Err(());
    }
    let entry = &mut manifest.entries[count];
    entry.role = role;
    entry.object_type = info.object_type as u8;
    entry.flags = 0;
    entry.rights = info.rights.bits();
    entry.handle = handle;
    entry.metadata = 0;
    manifest.entry_count = manifest.entry_count.checked_add(1).ok_or(())?;
    manifest.total_size = u32::from(manifest.header_size)
        .checked_add(u32::from(manifest.entry_size) * u32::from(manifest.entry_count))
        .ok_or(())?;
    process
        .update_bootstrap_manifest_size(manifest.total_size)
        .map_err(|_| ())?;
    manifest.validate().map_err(|_| ())
}

fn process_control_syscall(
    process_handle: gaxera_abi::Handle,
    caller_cspace_id: kernel_core::object::ObjectId,
    frame: &mut SyscallFrame,
) -> u64 {
    let (process_id, caller_domain_id) = {
        let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
        let system = match system_guard.as_mut() {
            Some(system) => system,
            None => return gaxera_abi::status::INTERNAL_ERROR,
        };
        let arena_guard = crate::global::OBJECT_ARENA.lock();
        let arena = match arena_guard.as_ref() {
            Some(arena) => arena,
            None => return gaxera_abi::status::INTERNAL_ERROR,
        };
        let cspaces = crate::global::CAPABILITY_SPACES.lock();
        let cspace = match cspaces.get(caller_cspace_id) {
            Some(cspace) => cspace,
            None => return gaxera_abi::status::INVALID_HANDLE,
        };
        let process_id = match system.lookup(
            cspace,
            process_handle,
            gaxera_abi::ObjectType::Process,
            gaxera_abi::Rights::MANAGE,
            arena,
        ) {
            Ok(id) => id,
            Err(_) => return gaxera_abi::status::RIGHTS_DENIED,
        };
        let caller_domain_id = cspace.domain();
        (process_id, caller_domain_id)
    };

    let operation = match gaxera_abi::ProcessControlOp::try_from(frame.rdx) {
        Ok(operation) => operation,
        Err(_) => return gaxera_abi::status::INVALID_ARGUMENT,
    };

    let components = {
        let processes = crate::global::PROCESSES.lock();
        let process = match processes.get(process_id) {
            Some(process) => process,
            None => return gaxera_abi::status::INVALID_HANDLE,
        };
        (
            process.state(),
            process.domain(),
            process.address_space(),
            process.capability_space(),
            process.main_thread(),
            process.exit_notification(),
            process.bootstrap_manifest(),
        )
    };

    match operation {
        gaxera_abi::ProcessControlOp::Query => {
            frame.rdx = process_state_code(components.0);
            frame.r10 = {
                let processes = crate::global::PROCESSES.lock();
                processes
                    .get(process_id)
                    .and_then(|process| process.exit_status())
                    .unwrap_or(0)
            };
            gaxera_abi::status::SUCCESS
        }
        gaxera_abi::ProcessControlOp::ConfigureMainThread => {
            #[cfg(feature = "test-process-create-start-exit")]
            if components.0 != kernel_core::process::ProcessState::Prepared {
                return gaxera_abi::status::INVALID_ARGUMENT;
            }
            let (address_space_id, cspace_id, thread_id) =
                match (components.2, components.3, components.4) {
                    (Some(address_space_id), Some(cspace_id), Some(thread_id)) => {
                        (address_space_id, cspace_id, thread_id)
                    }
                    _ => return gaxera_abi::status::INVALID_HANDLE,
                };
            if let Err(status) = configure_process_thread(
                thread_id,
                address_space_id,
                cspace_id,
                frame.r10,
                frame.r8,
                components.6,
            ) {
                return status;
            }
            let mut processes = crate::global::PROCESSES.lock();
            match processes.get_mut(process_id) {
                Some(process) => match process.configure_main_thread() {
                    Ok(()) => gaxera_abi::status::SUCCESS,
                    Err(_) => gaxera_abi::status::INVALID_ARGUMENT,
                },
                None => gaxera_abi::status::INVALID_HANDLE,
            }
        }
        gaxera_abi::ProcessControlOp::Start => {
            #[cfg(feature = "test-process-create-start-exit")]
            if components.0 != kernel_core::process::ProcessState::Prepared {
                return gaxera_abi::status::INVALID_ARGUMENT;
            }
            let thread_id = match components.4 {
                Some(id) => id,
                None => return gaxera_abi::status::INVALID_HANDLE,
            };
            // SAFETY: The process has not been published to the scheduler and
            // this syscall runs on the single BSP in the current architecture.
            let thread = match unsafe { crate::arch::x86_64::thread::THREADS.get_mut(thread_id) } {
                Some(thread) => thread,
                None => return gaxera_abi::status::INVALID_HANDLE,
            };
            if !components
                .0
                .eq(&kernel_core::process::ProcessState::Prepared)
            {
                return gaxera_abi::status::INVALID_ARGUMENT;
            }
            // Do not expose a half-started process: enqueue first, then make
            // the lifecycle transition. Roll back the queue entry if the
            // state transition unexpectedly fails.
            let cpu_local = unsafe { cpu::get_cpu_local() };
            let scheduler = unsafe { &mut *cpu_local.scheduler.get() }
                .as_mut()
                .ok_or(gaxera_abi::status::INTERNAL_ERROR);
            let scheduler = match scheduler {
                Ok(scheduler) => scheduler,
                Err(status) => return status,
            };
            if scheduler.enqueue(thread).is_err() {
                return gaxera_abi::status::RESOURCE_EXHAUSTED;
            }
            let transition = {
                let mut processes = crate::global::PROCESSES.lock();
                let process = match processes.get_mut(process_id) {
                    Some(process) if process.main_thread_configured() => process,
                    Some(_) => return gaxera_abi::status::INVALID_ARGUMENT,
                    None => return gaxera_abi::status::INVALID_HANDLE,
                };
                process.make_runnable()
            };
            if transition.is_err() {
                let _ = scheduler.remove_thread(thread_id);
                return gaxera_abi::status::INVALID_ARGUMENT;
            }

            gaxera_abi::status::SUCCESS
        }
        gaxera_abi::ProcessControlOp::Terminate => {
            let (main_thread_id, exit_notification) = {
                let mut processes = crate::global::PROCESSES.lock();
                let process = match processes.get_mut(process_id) {
                    Some(process) => process,
                    None => return gaxera_abi::status::INVALID_HANDLE,
                };
                if process.request_exit(frame.r10).is_err()
                    || process.mark_exiting().is_err()
                    || process.mark_zombie().is_err()
                {
                    return gaxera_abi::status::INVALID_ARGUMENT;
                }
                (process.main_thread(), process.exit_notification())
            };

            if let Some(thread_id) = main_thread_id {
                unsafe {
                    if let Some(thread) = crate::arch::x86_64::thread::THREADS.get_mut(thread_id) {
                        let _ = thread.make_dying();
                    }
                }
                let cpu_local = unsafe { cpu::get_cpu_local() };
                let scheduler = unsafe { &mut *cpu_local.scheduler.get() };
                if let Some(s) = scheduler.as_mut() {
                    let _ = s.remove_thread(thread_id);
                }
            }

            if let Some(notification_id) = exit_notification {
                let mut notifications = crate::global::NOTIFICATIONS.lock();
                if let Some(notification) = notifications.get_mut(notification_id) {
                    notification.signal(1);
                }
            }

            gaxera_abi::status::SUCCESS
        }
        gaxera_abi::ProcessControlOp::InstallCapability => install_process_capability(
            process_id,
            caller_cspace_id,
            components.3.unwrap_or(caller_cspace_id),
            components.1,
            gaxera_abi::Handle::from_raw(frame.r10),
            gaxera_abi::Rights::from_bits(frame.r8 as u32),
            frame.r9,
            frame,
        ),
        gaxera_abi::ProcessControlOp::AcquireAddressSpace
        | gaxera_abi::ProcessControlOp::AcquireMainThread
        | gaxera_abi::ProcessControlOp::AcquireCapabilitySpace
        | gaxera_abi::ProcessControlOp::AcquireResourceDomain
        | gaxera_abi::ProcessControlOp::Wait => {
            let (object, object_type, rights) = match operation {
                gaxera_abi::ProcessControlOp::AcquireAddressSpace => (
                    components.2,
                    gaxera_abi::ObjectType::AddressSpace,
                    gaxera_abi::Rights::MAP,
                ),
                gaxera_abi::ProcessControlOp::AcquireMainThread => (
                    components.4,
                    gaxera_abi::ObjectType::Thread,
                    gaxera_abi::Rights::MANAGE,
                ),
                gaxera_abi::ProcessControlOp::AcquireCapabilitySpace => (
                    components.3,
                    gaxera_abi::ObjectType::CapabilitySpace,
                    gaxera_abi::Rights::MANAGE,
                ),
                gaxera_abi::ProcessControlOp::AcquireResourceDomain => (
                    Some(components.1.object_id()),
                    gaxera_abi::ObjectType::ResourceDomain,
                    gaxera_abi::Rights::MANAGE,
                ),
                gaxera_abi::ProcessControlOp::Wait => (
                    None,
                    gaxera_abi::ObjectType::Notification,
                    gaxera_abi::Rights::WAIT,
                ),
                _ => unreachable!(),
            };
            if operation == gaxera_abi::ProcessControlOp::Wait {
                if components.0 == kernel_core::process::ProcessState::Zombie {
                    let processes = crate::global::PROCESSES.lock();
                    frame.rdx = processes
                        .get(process_id)
                        .and_then(|process| process.exit_status())
                        .unwrap_or(0);
                    return gaxera_abi::status::SUCCESS;
                }
                return gaxera_abi::status::TIMED_OUT;
            }
            let object = match object {
                Some(object) => object,
                None => return gaxera_abi::status::INVALID_HANDLE,
            };

            let mut domains = crate::global::RESOURCE_DOMAINS.lock();
            let caller_domain = match domains.get_mut(caller_domain_id.object_id()) {
                Some(domain) => domain,
                None => return gaxera_abi::status::INVALID_HANDLE,
            };
            let mut system_guard = crate::global::CAPABILITY_SYSTEM.lock();
            let system = match system_guard.as_mut() {
                Some(system) => system,
                None => return gaxera_abi::status::INTERNAL_ERROR,
            };
            let mut arena_guard = crate::global::OBJECT_ARENA.lock();
            let arena = match arena_guard.as_mut() {
                Some(arena) => arena,
                None => return gaxera_abi::status::INTERNAL_ERROR,
            };
            let mut cspaces = crate::global::CAPABILITY_SPACES.lock();
            let caller_cspace = match cspaces.get_mut(caller_cspace_id) {
                Some(cspace) => cspace,
                None => return gaxera_abi::status::INVALID_HANDLE,
            };
            match system.insert_root(
                caller_cspace,
                caller_domain,
                object,
                object_type,
                rights,
                arena,
            ) {
                Ok(handle) => {
                    frame.rdx = handle.raw();
                    gaxera_abi::status::SUCCESS
                }
                Err(_) => gaxera_abi::status::RESOURCE_EXHAUSTED,
            }
        }
        gaxera_abi::ProcessControlOp::Reap => {
            reap_process_syscall(process_id, caller_cspace_id, process_handle)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rollback_process_components(
    domains: &mut kernel_core::registry::BTreeRegistry<kernel_core::resource::ResourceDomain>,
    arena: &mut kernel_core::object::ObjectArena,
    child_domain: &mut kernel_core::resource::ResourceDomain,
    parent_domain_id: kernel_core::resource::ResourceDomainId,
    child_domain_id: kernel_core::resource::ResourceDomainId,
    process_id: kernel_core::object::ObjectId,
    aspace_id: kernel_core::object::ObjectId,
    cspace_id: kernel_core::object::ObjectId,
    thread_id: kernel_core::object::ObjectId,
    notification_id: kernel_core::object::ObjectId,
    child_aspace: &mut Option<(
        kernel_core::object::ObjectId,
        kernel_core::address_space::AddressSpace<
            crate::arch::x86_64::address_space::X86AddressSpace,
        >,
    )>,
) -> u64 {
    if let Some((_, aspace)) = child_aspace.take() {
        let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = physical.as_deref_mut() {
            let _ = aspace.arch.destroy(allocator);
        }
    }
    let _ = arena.destroy(child_domain, notification_id);
    let _ = arena.destroy(child_domain, thread_id);
    let _ = arena.destroy(child_domain, cspace_id);
    let _ = arena.destroy(child_domain, aspace_id);
    let _ = arena.destroy(child_domain, process_id);
    refund_child_domain(domains, child_domain);
    if let Some(parent) = domains.get_mut(parent_domain_id.object_id()) {
        let _ = arena.destroy(parent, child_domain_id.object_id());
    }
    gaxera_abi::status::RESOURCE_EXHAUSTED
}

#[allow(clippy::too_many_arguments)]
fn rollback_process_with_stack(
    stack: crate::arch::x86_64::stack::KernelStack,
    active_pt: &mut crate::arch::x86_64::paging::KernelPageTables,
    domains: &mut kernel_core::registry::BTreeRegistry<kernel_core::resource::ResourceDomain>,
    child_domain: &mut kernel_core::resource::ResourceDomain,
    parent_domain_id: kernel_core::resource::ResourceDomainId,
    child_domain_id: kernel_core::resource::ResourceDomainId,
    process_id: kernel_core::object::ObjectId,
    aspace_id: kernel_core::object::ObjectId,
    cspace_id: kernel_core::object::ObjectId,
    thread_id: kernel_core::object::ObjectId,
    notification_id: kernel_core::object::ObjectId,
    child_aspace: &mut Option<(
        kernel_core::object::ObjectId,
        kernel_core::address_space::AddressSpace<
            crate::arch::x86_64::address_space::X86AddressSpace,
        >,
    )>,
) -> u64 {
    {
        let mut physical = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = physical.as_deref_mut() {
            stack.reclaim(active_pt, allocator);
        }
    }
    let mut arena_guard = crate::global::OBJECT_ARENA.lock();
    match arena_guard.as_mut() {
        Some(arena) => rollback_process_components(
            domains,
            arena,
            child_domain,
            parent_domain_id,
            child_domain_id,
            process_id,
            aspace_id,
            cspace_id,
            thread_id,
            notification_id,
            child_aspace,
        ),
        None => gaxera_abi::status::INTERNAL_ERROR,
    }
}

fn refund_child_domain(
    domains: &mut kernel_core::registry::BTreeRegistry<kernel_core::resource::ResourceDomain>,
    child: &mut kernel_core::resource::ResourceDomain,
) {
    if let Some(parent_id) = child.parent()
        && let Some(parent) = domains.get_mut(parent_id.object_id())
    {
        let _ = child.refund_to_parent(parent);
    }
}

fn factory_create_syscall(
    factory_handle: gaxera_abi::Handle,
    caller_cspace_id: kernel_core::object::ObjectId,
    frame: &mut SyscallFrame,
) -> u64 {
    let obj_type = match gaxera_abi::ObjectType::try_from(frame.rdx as u32) {
        Ok(t) => t,
        Err(_) => {
            crate::println!("GAXERA: Invalid ObjectType {} in factory_create", frame.rdx);
            return u64::MAX;
        }
    };

    let (factory_id, is_image_factory) = {
        let mut cspaces = crate::global::CAPABILITY_SPACES.lock();
        let cspace = match cspaces.get_mut(caller_cspace_id) {
            Some(c) => c,
            None => return gaxera_abi::status::INVALID_HANDLE,
        };
        let mut system = crate::global::CAPABILITY_SYSTEM.lock();
        let sys = match system.as_mut() {
            Some(s) => s,
            None => return gaxera_abi::status::INTERNAL_ERROR,
        };
        let arena = crate::global::OBJECT_ARENA.lock();
        let arena_ref = match arena.as_ref() {
            Some(a) => a,
            None => return gaxera_abi::status::INTERNAL_ERROR,
        };
        let factory_info = match sys.lookup_info(
            cspace,
            factory_handle,
            gaxera_abi::ObjectType::Factory,
            gaxera_abi::Rights::FACTORY,
            arena_ref,
        ) {
            Ok(info) => info,
            Err(_) => return u64::MAX,
        };
        (
            factory_info.object,
            factory_info
                .rights
                .contains(gaxera_abi::Rights::IMAGE_FACTORY),
        )
    };

    let factories = crate::global::FACTORIES.lock();
    let factory = match factories.get(factory_id) {
        Some(f) => *f,
        None => return gaxera_abi::status::INVALID_HANDLE,
    };
    drop(factories);

    if !factory.allows(obj_type) {
        crate::println!("GAXERA: Factory denied obj_type {:?}", obj_type);
        return gaxera_abi::status::RIGHTS_DENIED;
    }

    let mut domains = crate::global::RESOURCE_DOMAINS.lock();
    let domain = match domains.get_mut(factory.domain().object_id()) {
        Some(d) => d,
        None => return gaxera_abi::status::INVALID_HANDLE,
    };
    let mut sys_lock = crate::global::CAPABILITY_SYSTEM.lock();
    let system = match sys_lock.as_mut() {
        Some(s) => s,
        None => return gaxera_abi::status::INTERNAL_ERROR,
    };
    let mut arena_lock = crate::global::OBJECT_ARENA.lock();
    let arena = match arena_lock.as_mut() {
        Some(a) => a,
        None => return gaxera_abi::status::INTERNAL_ERROR,
    };

    let mut pt_for_aspace = None;
    let mut stack_for_thread = None;
    let size = frame.r10;
    let mut mem_guard = None;
    let mut contiguous_guard = None;
    let mut interrupt_lease = None;

    if obj_type == gaxera_abi::ObjectType::MemoryObject {
        if size == 0 {
            return gaxera_abi::status::INVALID_ARGUMENT;
        }
        let num_frames = match size.checked_add(4095) {
            Some(sum) => (sum / 4096) as usize,
            None => return gaxera_abi::status::INVALID_ARGUMENT,
        };
        let rounded_bytes = match (num_frames as u64).checked_mul(4096) {
            Some(b) => b,
            None => return gaxera_abi::status::INVALID_ARGUMENT,
        };

        let mut phys = crate::global::PHYSICAL_ALLOCATOR.lock();
        let allocator = match phys.as_deref_mut() {
            Some(a) => a,
            None => return gaxera_abi::status::RESOURCE_EXHAUSTED,
        };

        let mut frames = alloc::vec::Vec::new();
        if frames.try_reserve_exact(num_frames).is_err() {
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
        if domain.charge_memory(rounded_bytes).is_err() {
            return gaxera_abi::status::MEMORY_LIMIT;
        }

        for _ in 0..num_frames {
            if let Some(f) = allocator.allocate_frame() {
                let vaddr = crate::memory::mapping::HHDM_BASE + f.start_address().as_u64();
                unsafe {
                    core::ptr::write_bytes(vaddr as *mut u8, 0, 4096);
                }
                frames.push(f.start_address().as_u64());
            } else {
                for f in &frames {
                    unsafe {
                        use x86_64::structures::paging::FrameDeallocator;
                        allocator.deallocate_frame(
                            x86_64::structures::paging::PhysFrame::containing_address(
                                x86_64::PhysAddr::new(*f),
                            ),
                        );
                    }
                }
                let _ = domain.rollback_memory(rounded_bytes);
                return gaxera_abi::status::RESOURCE_EXHAUSTED;
            }
        }
        mem_guard = Some((frames, rounded_bytes));
    } else if obj_type == gaxera_abi::ObjectType::ContiguousFrame {
        if size == 0 {
            return gaxera_abi::status::INVALID_ARGUMENT;
        }
        let page_count = match size
            .checked_add(crate::memory::physical::PAGE_SIZE - 1)
            .and_then(|bytes| usize::try_from(bytes / crate::memory::physical::PAGE_SIZE).ok())
        {
            Some(count) if count.is_power_of_two() => count,
            _ => return gaxera_abi::status::INVALID_ARGUMENT,
        };
        let rounded_bytes =
            match (page_count as u64).checked_mul(crate::memory::physical::PAGE_SIZE) {
                Some(bytes) => bytes,
                None => return gaxera_abi::status::INVALID_ARGUMENT,
            };
        if domain.charge_memory(rounded_bytes).is_err() {
            return gaxera_abi::status::MEMORY_LIMIT;
        }
        let mut phys = crate::global::PHYSICAL_ALLOCATOR.lock();
        let base = match phys
            .as_deref_mut()
            .and_then(|allocator| allocator.allocate_contiguous(page_count))
        {
            Some(base) => base,
            None => {
                let _ = domain.rollback_memory(rounded_bytes);
                return gaxera_abi::status::RESOURCE_EXHAUSTED;
            }
        };
        contiguous_guard = Some((base, page_count, rounded_bytes));
    } else if obj_type == gaxera_abi::ObjectType::AddressSpace
        || obj_type == gaxera_abi::ObjectType::Thread
    {
        let mut phys = crate::global::PHYSICAL_ALLOCATOR.lock();
        if let Some(allocator) = phys.as_deref_mut() {
            if obj_type == gaxera_abi::ObjectType::AddressSpace {
                match crate::arch::x86_64::address_space::X86AddressSpace::new_dynamic(allocator) {
                    Ok(a) => pt_for_aspace = Some(a),
                    Err(e) => {
                        crate::println!("GAXERA: AddressSpace::new_dynamic failed: {:?}", e);
                        return gaxera_abi::status::RESOURCE_EXHAUSTED;
                    }
                }
            } else if obj_type == gaxera_abi::ObjectType::Thread {
                let mut active_pt =
                    unsafe { crate::arch::x86_64::paging::KernelPageTables::active() };
                match crate::arch::x86_64::stack::KernelStack::allocate(&mut active_pt, allocator) {
                    Ok(s) => stack_for_thread = Some(s),
                    Err(e) => {
                        crate::println!("GAXERA: KernelStack::allocate failed: {:?}", e);
                        return gaxera_abi::status::RESOURCE_EXHAUSTED;
                    }
                }
            }
        } else {
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    }

    let new_id = match arena.create(domain, factory, obj_type) {
        Ok(id) => id,
        Err(e) => {
            crate::println!("GAXERA: arena.create error {:?}", e);
            if let Some((frames, rounded_bytes)) = mem_guard {
                let mut phys = crate::global::PHYSICAL_ALLOCATOR.lock();
                if let Some(allocator) = phys.as_deref_mut() {
                    for f in &frames {
                        unsafe {
                            use x86_64::structures::paging::FrameDeallocator;
                            allocator.deallocate_frame(
                                x86_64::structures::paging::PhysFrame::containing_address(
                                    x86_64::PhysAddr::new(*f),
                                ),
                            );
                        }
                    }
                }
                let _ = domain.rollback_memory(rounded_bytes);
            }
            if let Some((base, page_count, rounded_bytes)) = contiguous_guard {
                if let Some(allocator) = crate::global::PHYSICAL_ALLOCATOR.lock().as_deref_mut() {
                    let _ = allocator.deallocate_contiguous(base, page_count);
                }
                let _ = domain.rollback_memory(rounded_bytes);
            }
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    };

    if obj_type == gaxera_abi::ObjectType::InterruptObject {
        let irq = match u8::try_from(size) {
            Ok(irq) if irq < 16 => irq,
            _ => {
                let _ = arena.destroy(domain, new_id);
                return gaxera_abi::status::INVALID_ARGUMENT;
            }
        };
        match crate::arch::x86_64::interrupts::allocate(irq, new_id) {
            Ok(lease) => {
                crate::arch::x86_64::ioapic::ioapic_set_redirection(irq, lease.vector(), 0, true);
                interrupt_lease = Some(lease);
            }
            Err(_) => {
                let _ = arena.destroy(domain, new_id);
                return gaxera_abi::status::RESOURCE_EXHAUSTED;
            }
        }
    }

    let mut cspaces = crate::global::CAPABILITY_SPACES.lock();
    let target_cspace = match cspaces.get_mut(caller_cspace_id) {
        Some(c) => c,
        None => return gaxera_abi::status::INVALID_HANDLE,
    };

    let created_rights = if obj_type == gaxera_abi::ObjectType::MemoryObject {
        if is_image_factory {
            gaxera_abi::Rights::MAP
                | gaxera_abi::Rights::READ
                | gaxera_abi::Rights::WRITE
                | gaxera_abi::Rights::EXECUTE
                | gaxera_abi::Rights::MANAGE
        } else {
            gaxera_abi::Rights::MAP
                | gaxera_abi::Rights::READ
                | gaxera_abi::Rights::WRITE
                | gaxera_abi::Rights::MANAGE
        }
    } else if obj_type == gaxera_abi::ObjectType::InterruptObject {
        gaxera_abi::Rights::INTERRUPT
    } else if obj_type == gaxera_abi::ObjectType::ContiguousFrame {
        gaxera_abi::Rights::MAP | gaxera_abi::Rights::READ | gaxera_abi::Rights::WRITE
    } else {
        gaxera_abi::Rights::ALL
    };

    let new_handle = match system.insert_root(
        target_cspace,
        domain,
        new_id,
        obj_type,
        created_rights,
        arena,
    ) {
        Ok(handle) => handle,
        Err(e) => {
            crate::println!("GAXERA: system.insert_root error {:?}", e);
            let _ = arena.destroy(domain, new_id);
            if let Some((frames, rounded_bytes)) = mem_guard {
                let mut phys = crate::global::PHYSICAL_ALLOCATOR.lock();
                if let Some(allocator) = phys.as_deref_mut() {
                    for f in &frames {
                        unsafe {
                            use x86_64::structures::paging::FrameDeallocator;
                            allocator.deallocate_frame(
                                x86_64::structures::paging::PhysFrame::containing_address(
                                    x86_64::PhysAddr::new(*f),
                                ),
                            );
                        }
                    }
                }
                let _ = domain.rollback_memory(rounded_bytes);
            }
            if let Some((base, page_count, rounded_bytes)) = contiguous_guard {
                if let Some(allocator) = crate::global::PHYSICAL_ALLOCATOR.lock().as_deref_mut() {
                    let _ = allocator.deallocate_contiguous(base, page_count);
                }
                let _ = domain.rollback_memory(rounded_bytes);
            }
            return gaxera_abi::status::RESOURCE_EXHAUSTED;
        }
    };

    drop(cspaces);
    match obj_type {
        gaxera_abi::ObjectType::CapabilitySpace => {
            if let Ok(c) = kernel_core::capability::CapabilitySpace::try_new(domain, 64) {
                crate::global::CAPABILITY_SPACES.lock().insert(new_id, c);
            }
        }
        gaxera_abi::ObjectType::AddressSpace => {
            crate::global::ADDRESS_SPACES.lock().insert(
                new_id,
                kernel_core::address_space::AddressSpace::new(new_id, pt_for_aspace.unwrap()),
            );
        }
        gaxera_abi::ObjectType::Thread => {
            let arch = crate::arch::x86_64::thread::ArchThread {
                stack: stack_for_thread.unwrap(),
                context: crate::arch::x86_64::context::Context::empty(),
                cr3: None,
            };
            let thread = kernel_core::thread::Thread::new(new_id, None, arch);
            unsafe {
                crate::arch::x86_64::thread::THREADS.insert(thread);
            }
        }
        gaxera_abi::ObjectType::MemoryObject => {
            let (frames, rounded_bytes) = mem_guard.unwrap();
            let mut mem_obj = if is_image_factory {
                kernel_core::memory::MemoryObject::new_image(new_id, domain.id(), rounded_bytes)
            } else {
                kernel_core::memory::MemoryObject::new(new_id, domain.id(), rounded_bytes)
            };
            for f in frames {
                let _ = mem_obj.add_frame(f);
            }
            crate::global::MEMORY_OBJECTS.lock().insert(new_id, mem_obj);
        }
        gaxera_abi::ObjectType::ContiguousFrame => {
            let Some((phys_base, page_count, _)) = contiguous_guard.take() else {
                let _ = crate::arch::x86_64::teardown::delete_handle_internal(
                    caller_cspace_id,
                    new_handle,
                );
                return gaxera_abi::status::INTERNAL_ERROR;
            };
            let order = page_count.trailing_zeros() as u8;
            let frame_obj = kernel_core::contiguous_frame::ContiguousFrameObject::new(
                new_id,
                phys_base,
                page_count,
                order,
                domain.id(),
            );
            crate::global::CONTIGUOUS_FRAMES
                .lock()
                .insert(new_id, frame_obj);
        }
        gaxera_abi::ObjectType::Endpoint => {
            crate::global::ENDPOINTS
                .lock()
                .insert(new_id, kernel_core::ipc::Endpoint::new(new_id));
        }
        gaxera_abi::ObjectType::Notification => {
            crate::global::NOTIFICATIONS
                .lock()
                .insert(new_id, kernel_core::notification::Notification::new(new_id));
        }
        gaxera_abi::ObjectType::WaitSet => {
            crate::global::WAIT_SETS
                .lock()
                .insert(new_id, kernel_core::waitset::WaitSet::new(new_id));
        }
        gaxera_abi::ObjectType::TimerObject => {}
        gaxera_abi::ObjectType::DebugConsole => {
            crate::global::DEBUG_CONSOLES.lock().insert(
                new_id,
                kernel_core::debug_console::DebugConsole::new(new_id),
            );
        }
        gaxera_abi::ObjectType::Factory => {
            let child_factory =
                kernel_core::object::Factory::new_root(domain, gaxera_abi::ObjectTypeSet::ALL);
            crate::global::FACTORIES
                .lock()
                .insert(new_id, child_factory);
        }
        gaxera_abi::ObjectType::InterruptObject => {
            if let Some(lease) = interrupt_lease {
                let irq_obj = kernel_core::interrupt::InterruptObject::with_metadata(
                    new_id,
                    lease.vector(),
                    size as u8,
                    lease.generation(),
                    kernel_core::interrupt::InterruptTrigger::Level,
                    None,
                );
                crate::global::INTERRUPTS.lock().insert(new_id, irq_obj);
            }
        }
        _ => {}
    }

    frame.rdx = new_handle.raw();
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_frame() -> SyscallFrame {
        SyscallFrame {
            r15: 0,
            r14: 0,
            r13: 0,
            r12: 0,
            r11: 1 << 1,
            r10: 0,
            r9: 0,
            r8: 0,
            rbp: 0,
            rdi: 0,
            rsi: 0,
            rdx: 0,
            rcx: 0x1000,
            rax: 0,
            rsp: 0x2000,
        }
    }

    #[test]
    fn sysret_validation_rejects_hostile_return_addresses_and_flags() {
        let frame = valid_frame();
        assert!(validate_sysret_frame(&frame));
        assert!(!validate_sysret_frame(&SyscallFrame { rcx: 0, ..frame }));
        assert!(!validate_sysret_frame(&SyscallFrame {
            rsp: USER_ADDRESS_MAX + 1,
            ..frame
        }));
        assert!(!validate_sysret_frame(&SyscallFrame {
            r11: (1 << 1) | (3 << 12),
            ..frame
        }));
    }
}
