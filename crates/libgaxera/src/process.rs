use crate::syscall;
use alloc::vec::Vec;
use gaxera_abi::{Handle, ProcessControlOp, Rights};
use kernel_core::elf::parser::ElfParser;

#[derive(Debug)]
pub enum ProcessBuildError {
    ElfParseFailed,
    CreateProcessFailed,
    AcquireComponentFailed,
    MemoryAllocationFailed,
    TemporaryMappingFailed,
    ChildMappingFailed,
    StackMappingFailed,
    CapabilityInstallFailed,
    ThreadConfigurationFailed,
    StartFailed,
}

pub struct ProcessBuilder<'a> {
    factory: Handle,
    self_aspace: Handle,
    elf_data: &'a [u8],
    image_factory: Option<Handle>,
    capabilities: Vec<(u16, Handle, Rights)>,
}

impl<'a> ProcessBuilder<'a> {
    pub fn new(factory: Handle, self_aspace: Handle, elf_data: &'a [u8]) -> Self {
        Self {
            factory,
            self_aspace,
            elf_data,
            image_factory: None,
            capabilities: Vec::new(),
        }
    }

    pub fn with_image_factory(mut self, image_factory: Handle) -> Self {
        self.image_factory = Some(image_factory);
        self
    }

    pub fn install_capability(
        mut self,
        slot: u16,
        handle: Handle,
        rights: Rights,
    ) -> Result<Self, ProcessBuildError> {
        self.capabilities
            .try_reserve(1)
            .map_err(|_| ProcessBuildError::MemoryAllocationFailed)?;
        self.capabilities.push((slot, handle, rights));
        Ok(self)
    }

    pub fn spawn(self) -> Result<Handle, ProcessBuildError> {
        let parser =
            ElfParser::new(self.elf_data).map_err(|_| ProcessBuildError::ElfParseFailed)?;

        // 1. Create Process
        // Keep the initial child reservation bounded. The supervisor can
        // raise quotas through an explicit ResourceDomain policy later; a
        // hard-coded 64 MiB reservation makes small bootstrap tests fail
        // against the parent's finite quota.
        let process_handle = syscall::create_process(self.factory, 128, 128, 4 * 1024 * 1024)
            .map_err(|_| ProcessBuildError::CreateProcessFailed)?;

        // 2. Acquire control handles. The child CSpace is also acquired so
        // capability installation remains explicit and slot-independent.
        let aspace_raw = match syscall::process_control(
            process_handle,
            ProcessControlOp::AcquireAddressSpace,
            0,
            0,
            0,
        ) {
            Ok(h) => h,
            Err(_) => {
                let _ = syscall::delete_handle(process_handle);
                return Err(ProcessBuildError::AcquireComponentFailed);
            }
        };
        let aspace_handle = Handle::from_raw(aspace_raw);

        let thread_raw = match syscall::process_control(
            process_handle,
            ProcessControlOp::AcquireMainThread,
            0,
            0,
            0,
        ) {
            Ok(h) => h,
            Err(_) => {
                let _ = syscall::delete_handle(aspace_handle);
                let _ = syscall::delete_handle(process_handle);
                return Err(ProcessBuildError::AcquireComponentFailed);
            }
        };
        let thread_handle = Handle::from_raw(thread_raw);
        let child_cspace = match syscall::process_capability_space(process_handle) {
            Ok(handle) => handle,
            Err(_) => {
                let _ = syscall::delete_handle(thread_handle);
                let _ = syscall::delete_handle(aspace_handle);
                let _ = syscall::delete_handle(process_handle);
                return Err(ProcessBuildError::AcquireComponentFailed);
            }
        };

        let img_factory = self.image_factory.unwrap_or(self.factory);

        // 3. Load PT_LOAD Segments
        let temp_vaddr = 0x0000_5000_0000_0000_u64;
        for ph in parser.program_headers() {
            if ph.p_type == kernel_core::elf::types::PT_LOAD {
                let is_exec = (ph.p_flags & kernel_core::elf::types::PF_X) != 0;
                let is_write = (ph.p_flags & kernel_core::elf::types::PF_W) != 0;

                // Validate W^X rule
                if is_exec && is_write {
                    let _ = syscall::delete_handle(thread_handle);
                    let _ = syscall::delete_handle(aspace_handle);
                    let _ = syscall::delete_handle(process_handle);
                    return Err(ProcessBuildError::ElfParseFailed);
                }

                let target_factory = if is_exec { img_factory } else { self.factory };

                let segment_offset = ph.p_vaddr & 4095;
                let map_vaddr = ph.p_vaddr & !4095;
                let mem_size = segment_offset
                    .checked_add(ph.p_memsz)
                    .and_then(|size| size.checked_add(4095))
                    .ok_or(ProcessBuildError::MemoryAllocationFailed)?
                    & !4095;
                if mem_size == 0 {
                    continue;
                }

                let mem_handle =
                    match syscall::factory_create_memory_object(target_factory, mem_size) {
                        Ok(h) => h,
                        Err(_) => {
                            let _ = syscall::delete_handle(thread_handle);
                            let _ = syscall::delete_handle(aspace_handle);
                            let _ = syscall::delete_handle(process_handle);
                            return Err(ProcessBuildError::MemoryAllocationFailed);
                        }
                    };

                // Temporary map into supervisor space to copy ELF segment file payload
                let temp_mapping = match syscall::map_memory(
                    self.self_aspace,
                    mem_handle,
                    temp_vaddr,
                    Rights::READ | Rights::WRITE,
                ) {
                    Ok(h) => h,
                    Err(_) => {
                        let _ = syscall::delete_handle(mem_handle);
                        let _ = syscall::delete_handle(thread_handle);
                        let _ = syscall::delete_handle(aspace_handle);
                        let _ = syscall::delete_handle(process_handle);
                        return Err(ProcessBuildError::TemporaryMappingFailed);
                    }
                };

                let filesz = ph.p_filesz as usize;
                let offset = ph.p_offset as usize;
                let destination_offset = segment_offset as usize;
                if filesz > 0
                    && offset
                        .checked_add(filesz)
                        .is_some_and(|end| end <= self.elf_data.len())
                    && destination_offset
                        .checked_add(filesz)
                        .is_some_and(|end| end <= mem_size as usize)
                {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            self.elf_data[offset..offset + filesz].as_ptr(),
                            (temp_vaddr + segment_offset) as *mut u8,
                            filesz,
                        );
                    }
                }

                let _ = syscall::unmap_memory(temp_mapping);
                let _ = syscall::delete_handle(temp_mapping);

                let mut segment_rights = Rights::READ;
                if is_write {
                    segment_rights |= Rights::WRITE;
                }
                if is_exec {
                    segment_rights |= Rights::EXECUTE;
                }

                let _child_mapping =
                    match syscall::map_memory(aspace_handle, mem_handle, map_vaddr, segment_rights)
                    {
                        Ok(h) => h,
                        Err(_) => {
                            let _ = syscall::delete_handle(mem_handle);
                            let _ = syscall::delete_handle(thread_handle);
                            let _ = syscall::delete_handle(aspace_handle);
                            let _ = syscall::delete_handle(process_handle);
                            return Err(ProcessBuildError::ChildMappingFailed);
                        }
                    };
            }
        }

        // 4. Create Guarded Stack (64 KiB stack)
        let stack_size = 64 * 1024;
        let stack_vaddr = 0x0000_7FFF_FE00_0000_u64;
        let stack_mem = match syscall::factory_create_memory_object(self.factory, stack_size) {
            Ok(h) => h,
            Err(_) => {
                let _ = syscall::delete_handle(thread_handle);
                let _ = syscall::delete_handle(aspace_handle);
                let _ = syscall::delete_handle(process_handle);
                return Err(ProcessBuildError::MemoryAllocationFailed);
            }
        };

        let stack_rights = Rights::READ | Rights::WRITE;
        if syscall::map_memory(aspace_handle, stack_mem, stack_vaddr, stack_rights).is_err() {
            let _ = syscall::delete_handle(stack_mem);
            let _ = syscall::delete_handle(thread_handle);
            let _ = syscall::delete_handle(aspace_handle);
            let _ = syscall::delete_handle(process_handle);
            return Err(ProcessBuildError::StackMappingFailed);
        }

        let stack_top = stack_vaddr + stack_size;

        // 5. Install Child Capabilities
        for (slot, cap_handle, cap_rights) in self.capabilities {
            if syscall::process_control(
                process_handle,
                ProcessControlOp::InstallCapability,
                cap_handle.raw(),
                cap_rights.bits() as u64,
                slot as u64,
            )
            .is_err()
            {
                let _ = syscall::delete_handle(child_cspace);
                let _ = syscall::delete_handle(thread_handle);
                let _ = syscall::delete_handle(aspace_handle);
                let _ = syscall::delete_handle(process_handle);
                return Err(ProcessBuildError::CapabilityInstallFailed);
            }
        }

        // 6. Configure Main Thread
        let entry_point = parser.entry_point();
        if syscall::process_control(
            process_handle,
            ProcessControlOp::ConfigureMainThread,
            entry_point,
            stack_top,
            0,
        )
        .is_err()
        {
            let _ = syscall::delete_handle(thread_handle);
            let _ = syscall::delete_handle(aspace_handle);
            let _ = syscall::delete_handle(process_handle);
            return Err(ProcessBuildError::ThreadConfigurationFailed);
        }

        // 7. Start Process
        if syscall::process_control(process_handle, ProcessControlOp::Start, 0, 0, 0).is_err() {
            let _ = syscall::delete_handle(thread_handle);
            let _ = syscall::delete_handle(aspace_handle);
            let _ = syscall::delete_handle(process_handle);
            return Err(ProcessBuildError::StartFailed);
        }

        let _ = syscall::delete_handle(child_cspace);
        let _ = syscall::delete_handle(thread_handle);
        let _ = syscall::delete_handle(aspace_handle);

        Ok(process_handle)
    }
}
