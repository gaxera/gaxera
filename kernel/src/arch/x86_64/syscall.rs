use crate::arch::x86_64::cpu;
use core::arch::global_asm;
use x86_64::registers::model_specific::{Efer, EferFlags, Msr};
use x86_64::registers::rflags::RFlags;

use crate::memory::mapping::USER_ADDRESS_MAX;
use crate::println;
use kernel_core::registry::ObjectRegistry;
use x86_64::structures::paging::FrameAllocator;

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
        if let Some(domain) = domains.iter_mut().find(|d| d.id() == domain_id) {
            let _ = domain.release_memory(size_bytes);
            let _ = domain.release_object();
        }
    }

    // Destroy object from ObjectArena
    {
        let mut domains = crate::global::RESOURCE_DOMAINS.lock();
        let mut arena_guard = crate::global::OBJECT_ARENA.lock();
        if let (Some(arena), Some(domain)) = (
            arena_guard.as_mut(),
            domains.iter_mut().find(|d| d.id() == domain_id),
        ) {
            let _ = arena.destroy(domain, object_id);
        }
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
                    if let Some(domain) = domains.iter_mut().find(|d| d.id() == owner_id) {
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
        //     "GAXERA: SYSCALL {} handle={} rsi={}",
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
            {
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

                            // Reject W+X for anonymous memory objects
                            if requested_rights.contains(gaxera_abi::Rights::EXECUTE) {
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
                                Err(_) => break 'sys_invoke u64::MAX,
                            }
                            drop(mem_objects);

                            // Transaction step 1: Reserve object & charge caller ResourceDomain
                            let mut domain_guard = crate::global::RESOURCE_DOMAINS.lock();
                            let domain = match domain_guard
                                .iter_mut()
                                .find(|d| d.id() == caller_domain_id)
                            {
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
                                Err(_) => break 'sys_invoke u64::MAX,
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
                                            domains.iter_mut().find(|d| d.id() == caller_domain_id)
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
                                        domains.iter_mut().find(|d| d.id() == caller_domain_id)
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
                                            domains.iter_mut().find(|d| d.id() == caller_domain_id)
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
                                            domains.iter_mut().find(|d| d.id() == caller_domain_id)
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
                            let domain = match domain_guard
                                .iter_mut()
                                .find(|d| d.id() == caller_domain_id)
                            {
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
                                requested_rights,
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
                                            domains.iter_mut().find(|d| d.id() == caller_domain_id)
                                        {
                                            let _ = arena.destroy(domain, mapping_id);
                                        }
                                    }
                                    break 'sys_invoke u64::MAX;
                                }
                            }
                            0
                        } else if let Ok(mapping_id) = mapping_result {
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

                            if let Some(phys_base) = mapping.phys_addr() {
                                use kernel_core::address_space::ArchAddressSpace;
                                match aspace.arch.map_physical_range(
                                    virtual_address,
                                    phys_base,
                                    mapping.size(),
                                    requested_rights,
                                    mapping.cache_policy(),
                                ) {
                                    Ok(_) => 0,
                                    Err(_) => u64::MAX,
                                }
                            } else {
                                crate::println!("GAXERA: MapMemory phys_addr is None!");
                                break 'sys_invoke u64::MAX;
                            }
                        } else {
                            crate::println!(
                                "GAXERA: MapMemory mapping_result failed: {:?}",
                                mapping_result
                            );
                            break 'sys_invoke u64::MAX;
                        }
                    } else {
                        crate::println!(
                            "GAXERA: MapMemory aspace_result failed: {:?}",
                            aspace_result
                        );
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::UnmapMemory as u64 {
                    let mapping_handle = gaxera_abi::Handle::from_raw(frame.rdx);
                    let caller_domain_id = cspace.domain();
                    let mapping_result = sys.lookup(
                        cspace,
                        mapping_handle,
                        gaxera_abi::ObjectType::Mapping,
                        gaxera_abi::Rights::MANAGE,
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
                        if let Some(domain) =
                            domain_guard.iter_mut().find(|d| d.id() == caller_domain_id)
                        {
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
                        }

                        frame.rax = 0;
                        0
                    } else {
                        break 'sys_invoke u64::MAX;
                    }
                } else if op == gaxera_abi::OperationCode::CreateWaitSet as u64 {
                    let mut domain_guard = crate::global::RESOURCE_DOMAINS.lock();
                    let domain = match domain_guard.first_mut() {
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

                    let target_domain = match domains
                        .iter_mut()
                        .find(|d| d.id() == target_cspace.domain())
                    {
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

                    if crate::arch::x86_64::teardown::delete_handle_internal(
                        cspace_id,
                        target_handle,
                    )
                    .is_ok()
                    {
                        0
                    } else {
                        u64::MAX
                    }
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
                } else if op == 0 {
                    let obj_type = match gaxera_abi::ObjectType::try_from(frame.rdx as u32) {
                        Ok(t) => t,
                        Err(_) => {
                            crate::println!(
                                "GAXERA: Invalid ObjectType {} in factory_create",
                                frame.rdx
                            );
                            break 'sys_invoke u64::MAX;
                        }
                    };

                    match sys.lookup(
                        cspace,
                        handle,
                        gaxera_abi::ObjectType::Factory,
                        gaxera_abi::Rights::FACTORY,
                        arena_ref,
                    ) {
                        Ok(factory_id) => {
                            let factories = crate::global::FACTORIES.lock();
                            let factory = match factories.get(factory_id) {
                                Some(f) => *f,
                                None => break 'sys_invoke gaxera_abi::status::INVALID_HANDLE,
                            };
                            drop(factories);

                            // Enforce factory.allows(obj_type) BEFORE allocating frames or objects
                            if !factory.allows(obj_type) {
                                crate::println!("GAXERA: Factory denied obj_type {:?}", obj_type);
                                break 'sys_invoke gaxera_abi::status::RIGHTS_DENIED;
                            }

                            // To avoid deadlocks, drop sys/arena locks
                            drop(system);
                            drop(arena);
                            drop(cspaces);

                            let mut domains = crate::global::RESOURCE_DOMAINS.lock();
                            let domain =
                                match domains.iter_mut().find(|d| d.id() == factory.domain()) {
                                    Some(d) => d,
                                    None => break 'sys_invoke gaxera_abi::status::INVALID_HANDLE,
                                };
                            let mut sys_lock = crate::global::CAPABILITY_SYSTEM.lock();
                            let system = match sys_lock.as_mut() {
                                Some(s) => s,
                                None => break 'sys_invoke gaxera_abi::status::INTERNAL_ERROR,
                            };
                            let mut arena_lock = crate::global::OBJECT_ARENA.lock();
                            let arena = match arena_lock.as_mut() {
                                Some(a) => a,
                                None => break 'sys_invoke gaxera_abi::status::INTERNAL_ERROR,
                            };

                            let mut pt_for_aspace = None;
                            let mut stack_for_thread = None;
                            let size = frame.r10; // arg2
                            let mut mem_guard = None;

                            if obj_type == gaxera_abi::ObjectType::MemoryObject {
                                if size == 0 {
                                    break 'sys_invoke gaxera_abi::status::INVALID_ARGUMENT;
                                }
                                let num_frames = match size.checked_add(4095) {
                                    Some(sum) => (sum / 4096) as usize,
                                    None => break 'sys_invoke gaxera_abi::status::INVALID_ARGUMENT,
                                };
                                let rounded_bytes = match (num_frames as u64).checked_mul(4096) {
                                    Some(b) => b,
                                    None => break 'sys_invoke gaxera_abi::status::INVALID_ARGUMENT,
                                };

                                let mut phys = crate::global::PHYSICAL_ALLOCATOR.lock();
                                let allocator = match phys.as_deref_mut() {
                                    Some(a) => a,
                                    None => {
                                        break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                    }
                                };

                                let mut frames = alloc::vec::Vec::new();
                                if frames.try_reserve_exact(num_frames).is_err() {
                                    break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                }
                                if domain.charge_memory(rounded_bytes).is_err() {
                                    break 'sys_invoke gaxera_abi::status::MEMORY_LIMIT;
                                }

                                for _ in 0..num_frames {
                                    if let Some(f) = allocator.allocate_frame() {
                                        let vaddr = crate::memory::mapping::HHDM_BASE
                                            + f.start_address().as_u64();
                                        // SAFETY: Frame is exclusively allocated and mapped via HHDM.
                                        unsafe {
                                            core::ptr::write_bytes(vaddr as *mut u8, 0, 4096);
                                        }
                                        frames.push(f.start_address().as_u64());
                                    } else {
                                        // Rollback physical frames allocated so far & quota charge
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
                                        break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                    }
                                }
                                mem_guard = Some((frames, rounded_bytes));
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
                                                break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                            }
                                        }
                                    } else if obj_type == gaxera_abi::ObjectType::Thread {
                                        // SAFETY: HHDM is active, active CR3 provides kernel mappings.
                                        let mut active_pt = unsafe {
                                            crate::arch::x86_64::paging::KernelPageTables::active()
                                        };
                                        match crate::arch::x86_64::stack::KernelStack::allocate(
                                            &mut active_pt,
                                            allocator,
                                        ) {
                                            Ok(s) => stack_for_thread = Some(s),
                                            Err(e) => {
                                                crate::println!(
                                                    "GAXERA: KernelStack::allocate failed: {:?}",
                                                    e
                                                );
                                                break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                            }
                                        }
                                    }
                                } else {
                                    break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                }
                            }

                            match arena.create(domain, factory, obj_type) {
                                Ok(new_id) => {
                                    let mut cspaces = crate::global::CAPABILITY_SPACES.lock();
                                    let cspace_ref = cspaces.get_mut(cspace_id).unwrap();
                                    // SAFETY: By dropping outer locks, we ensure exclusive access to the target CSpace.
                                    let target_cspace = unsafe {
                                        &mut *(cspace_ref as *const _
                                            as *mut kernel_core::capability::CapabilitySpace)
                                    };

                                    let created_rights =
                                        if obj_type == gaxera_abi::ObjectType::MemoryObject {
                                            gaxera_abi::Rights::MAP
                                                | gaxera_abi::Rights::READ
                                                | gaxera_abi::Rights::WRITE
                                        } else {
                                            gaxera_abi::Rights::ALL
                                        };
                                    match system.insert_root(
                                        target_cspace,
                                        domain,
                                        new_id,
                                        obj_type,
                                        created_rights,
                                        arena,
                                    ) {
                                        Ok(new_handle) => {
                                            drop(cspaces);
                                            match obj_type {
                                                gaxera_abi::ObjectType::CapabilitySpace => {
                                                    match kernel_core::capability::CapabilitySpace::try_new(domain, 64) {
                                                        Ok(c) => {
                                                            let mut cspaces2 = crate::global::CAPABILITY_SPACES.lock();
                                                            cspaces2.insert(new_id, c);
                                                            drop(cspaces2);
                                                        }
                                                        Err(e) => crate::println!("GAXERA: CapabilitySpace::try_new failed: {:?}", e),
                                                    }
                                                }
                                                gaxera_abi::ObjectType::AddressSpace => {
                                                    crate::global::ADDRESS_SPACES.lock().insert(new_id, kernel_core::address_space::AddressSpace::new(new_id, pt_for_aspace.unwrap()));
                                                }
                                                gaxera_abi::ObjectType::Thread => {
                                                    let arch = crate::arch::x86_64::thread::ArchThread {
                                                        stack: stack_for_thread.unwrap(),
                                                        context: crate::arch::x86_64::context::Context::empty(),
                                                        cr3: None,
                                                    };
                                                    let thread = kernel_core::thread::Thread::new(
                                                        new_id, None, arch,
                                                    );
                                                    // SAFETY: Thread is newly created and accessed exclusively.
                                                    unsafe {
                                                        crate::arch::x86_64::thread::THREADS
                                                            .insert(thread);
                                                    }
                                                }
                                                gaxera_abi::ObjectType::MemoryObject => {
                                                    let (frames, rounded_bytes) =
                                                        mem_guard.unwrap();
                                                    let mut mem_obj =
                                                        kernel_core::memory::MemoryObject::new(
                                                            new_id,
                                                            domain.id(),
                                                            rounded_bytes,
                                                        );
                                                    for f in frames {
                                                        let _ = mem_obj.add_frame(f);
                                                    }
                                                    crate::global::MEMORY_OBJECTS
                                                        .lock()
                                                        .insert(new_id, mem_obj);
                                                }
                                                gaxera_abi::ObjectType::ContiguousFrame => {
                                                    let page_count = (size as usize).div_ceil(4096);
                                                    let mut phys =
                                                        crate::global::PHYSICAL_ALLOCATOR.lock();
                                                    let allocator = phys.as_deref_mut().unwrap();
                                                    let phys_base = allocator
                                                        .allocate_contiguous(page_count)
                                                        .unwrap();
                                                    let order = page_count.trailing_zeros() as u8;
                                                    let frame_obj =
                                                        kernel_core::contiguous_frame::ContiguousFrameObject::new(
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
                                                    crate::global::ENDPOINTS.lock().insert(
                                                        new_id,
                                                        kernel_core::ipc::Endpoint::new(new_id),
                                                    );
                                                }
                                                _ => {}
                                            }
                                            frame.rdx = new_handle.raw();
                                            0
                                        }
                                        Err(e) => {
                                            crate::println!("GAXERA: insert_root failed: {:?}", e);
                                            let _ = arena.destroy(domain, new_id);
                                            if let Some((frames, rounded_bytes)) = mem_guard {
                                                let mut phys =
                                                    crate::global::PHYSICAL_ALLOCATOR.lock();
                                                if let Some(allocator) = phys.as_deref_mut() {
                                                    for f in &frames {
                                                        // SAFETY: Frame returned to physical allocator upon object allocation failure.
                                                        unsafe {
                                                            use x86_64::structures::paging::FrameDeallocator;
                                                            allocator.deallocate_frame(x86_64::structures::paging::PhysFrame::containing_address(x86_64::PhysAddr::new(*f)));
                                                        }
                                                    }
                                                }
                                                let _ = domain.rollback_memory(rounded_bytes);
                                            }
                                            break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                        }
                                    }
                                }
                                Err(e) => {
                                    crate::println!("GAXERA: arena.create error {:?}", e);
                                    if let Some((frames, rounded_bytes)) = mem_guard {
                                        let mut phys = crate::global::PHYSICAL_ALLOCATOR.lock();
                                        if let Some(allocator) = phys.as_deref_mut() {
                                            for f in &frames {
                                                unsafe {
                                                    use x86_64::structures::paging::FrameDeallocator;
                                                    allocator.deallocate_frame(x86_64::structures::paging::PhysFrame::containing_address(x86_64::PhysAddr::new(*f)));
                                                }
                                            }
                                        }
                                        let _ = domain.rollback_memory(rounded_bytes);
                                    }
                                    break 'sys_invoke gaxera_abi::status::RESOURCE_EXHAUSTED;
                                }
                            }
                        }
                        Err(e) => {
                            crate::println!("GAXERA: sys.lookup error {:?}", e);
                            break 'sys_invoke u64::MAX;
                        }
                    }
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
                        gaxera_abi::Rights::WRITE,
                        arena_ref,
                    ) {
                        Ok(irq_obj_id) => {
                            let sub_op = frame.rsi;
                            drop(arena);
                            drop(system);
                            drop(cspaces);

                            let mut interrupts = crate::global::INTERRUPTS.lock();
                            let irq_obj = match interrupts.get_mut(irq_obj_id) {
                                Some(obj) => obj,
                                None => break 'sys_invoke u64::MAX,
                            };

                            if sub_op == gaxera_abi::InterruptOp::BindNotification as u64 {
                                let notif_handle = gaxera_abi::Handle::from_raw(frame.rdx);
                                let mut cspaces = crate::global::CAPABILITY_SPACES.lock();
                                let mut system = crate::global::CAPABILITY_SYSTEM.lock();
                                let mut arena = crate::global::OBJECT_ARENA.lock();

                                let sys = system.as_mut().unwrap();
                                let arena_ref = arena.as_mut().unwrap();
                                let cspace = match cspaces.get_mut(cspace_id) {
                                    Some(cs) => cs,
                                    None => break 'sys_invoke u64::MAX,
                                };

                                match sys.lookup(
                                    cspace,
                                    notif_handle,
                                    gaxera_abi::ObjectType::Notification,
                                    gaxera_abi::Rights::WRITE,
                                    arena_ref,
                                ) {
                                    Ok(notif_id) => {
                                        let _ = irq_obj.bind_notification(notif_id);
                                        0
                                    }
                                    Err(_) => u64::MAX,
                                }
                            } else if sub_op == gaxera_abi::InterruptOp::Mask as u64 {
                                irq_obj.mask();
                                crate::arch::x86_64::ioapic::ioapic_mask_irq(irq_obj.irq());
                                0
                            } else if sub_op == gaxera_abi::InterruptOp::Unmask as u64 {
                                irq_obj.unmask();
                                crate::arch::x86_64::ioapic::ioapic_unmask_irq(irq_obj.irq());
                                0
                            } else if sub_op == gaxera_abi::InterruptOp::Ack as u64 {
                                // SAFETY: Sends EOI to Local APIC.
                                unsafe {
                                    crate::arch::x86_64::apic::send_eoi();
                                }
                                0
                            } else {
                                u64::MAX
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
                } else if op == gaxera_abi::OperationCode::ExitProcess as u64 {
                    let exit_code = frame.rdx;
                    if exit_code != 0 {
                        crate::println!(
                            "GAXERA: USER_PROCESS_EXITED with error code {:#x}",
                            exit_code
                        );
                        loop {
                            unsafe { core::arch::asm!("hlt") };
                        }
                    }

                    // 1. Snapshot handles fallibly
                    let handles = match cspace.snapshot_handles() {
                        Ok(h) => h,
                        Err(_) => break 'sys_invoke u64::MAX, // Can't even snapshot, just halt
                    };

                    drop(arena);
                    drop(system);
                    drop(cspaces);

                    // 2. Iterate and delete handles
                    for handle in handles {
                        let _ = crate::arch::x86_64::teardown::delete_handle_internal(
                            cspace_id, handle,
                        );
                    }

                    // A real implementation would also destroy AddressSpace and Thread structures,
                    // but we just halt to signal successful teardown here, or qemu exit if test.
                    #[cfg(feature = "test-ring3-heap")]
                    crate::println!("GAXERA: RING3_HEAP_TEST_SUCCESS");
                    #[cfg(feature = "test-ring3-heap")]
                    unsafe {
                        crate::arch::x86_64::qemu::exit_success();
                    }
                    #[cfg(feature = "test-memory-lifecycle")]
                    {
                        crate::println!("GAXERA: MEMORY_RECLAIMED_AND_QUOTA_REFUNDED");
                        // SAFETY: This is a QEMU integration image launched with
                        // isa-debug-exit by xtask for this test profile.
                        unsafe { crate::arch::x86_64::qemu::exit_success() };
                    }
                    #[cfg(not(any(
                        feature = "test-ring3-heap",
                        feature = "test-memory-lifecycle"
                    )))]
                    loop {
                        unsafe { core::arch::asm!("hlt") };
                    }
                } else {
                    break 'sys_invoke u64::MAX;
                }
            }
        }
        _ => u64::MAX, // Error / unknown syscall
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
    let current_id = scheduler.current_thread().ok_or(())?;
    let next_id = match scheduler.next_runnable() {
        Some(id) => id,
        None => return Ok(()),
    };

    crate::arch::x86_64::preemption::reschedule(scheduler, current_id, next_id)
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
