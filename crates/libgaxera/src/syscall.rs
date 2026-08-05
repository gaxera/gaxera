use gaxera_abi::ipc::InlineMessage;
use gaxera_abi::{Handle, InterruptOp, OperationCode, WaitSetEvent};

use crate::arch::raw_syscall;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SyscallError {
    InvalidHandle,
    RightsDenied,
    InvalidArgument,
    ResourceExhausted,
    TimedOut,
    MappingCollision,
    ObjectLimit,
    CapabilityLimit,
    MemoryLimit,
    NotSupported,
    InternalError,
    Unknown(u64),
}

impl SyscallError {
    pub fn from_code(code: u64) -> Self {
        match code {
            gaxera_abi::status::INVALID_HANDLE => Self::InvalidHandle,
            gaxera_abi::status::RIGHTS_DENIED => Self::RightsDenied,
            gaxera_abi::status::INVALID_ARGUMENT => Self::InvalidArgument,
            gaxera_abi::status::RESOURCE_EXHAUSTED => Self::ResourceExhausted,
            gaxera_abi::status::TIMED_OUT => Self::TimedOut,
            gaxera_abi::status::MAPPING_COLLISION => Self::MappingCollision,
            gaxera_abi::status::OBJECT_LIMIT => Self::ObjectLimit,
            gaxera_abi::status::CAPABILITY_LIMIT => Self::CapabilityLimit,
            gaxera_abi::status::MEMORY_LIMIT => Self::MemoryLimit,
            gaxera_abi::status::NOT_SUPPORTED => Self::NotSupported,
            gaxera_abi::status::INTERNAL_ERROR => Self::InternalError,
            other => Self::Unknown(other),
        }
    }

    pub fn to_code(self) -> u64 {
        match self {
            Self::InvalidHandle => gaxera_abi::status::INVALID_HANDLE,
            Self::RightsDenied => gaxera_abi::status::RIGHTS_DENIED,
            Self::InvalidArgument => gaxera_abi::status::INVALID_ARGUMENT,
            Self::ResourceExhausted => gaxera_abi::status::RESOURCE_EXHAUSTED,
            Self::TimedOut => gaxera_abi::status::TIMED_OUT,
            Self::MappingCollision => gaxera_abi::status::MAPPING_COLLISION,
            Self::ObjectLimit => gaxera_abi::status::OBJECT_LIMIT,
            Self::CapabilityLimit => gaxera_abi::status::CAPABILITY_LIMIT,
            Self::MemoryLimit => gaxera_abi::status::MEMORY_LIMIT,
            Self::NotSupported => gaxera_abi::status::NOT_SUPPORTED,
            Self::InternalError => gaxera_abi::status::INTERNAL_ERROR,
            Self::Unknown(code) => code,
        }
    }
}

/// Decode a raw status code into a Result<(), SyscallError>.
pub fn decode_status(code: u64) -> Result<(), SyscallError> {
    if code == gaxera_abi::status::SUCCESS {
        Ok(())
    } else {
        Err(SyscallError::from_code(code))
    }
}

/// Dedicated syscall wrapper for MemoryObject creation using a Factory capability.
///
/// Returns `Ok(Handle)` on success, or `Err(SyscallError)` on failure.
/// Because %rax carries status and %rdx carries the returned handle,
/// a valid handle value can never be mistaken for a syscall error code.
pub fn factory_create_memory_object(
    factory: Handle,
    size_bytes: u64,
) -> Result<Handle, SyscallError> {
    let (status, raw_handle) = unsafe {
        raw_syscall::raw_syscall6_ret2(
            factory.raw(),
            OperationCode::FactoryCreate as u64,
            gaxera_abi::ObjectType::MemoryObject as u64,
            size_bytes,
            0,
            0,
        )
    };
    decode_status(status)?;
    Ok(Handle::from_raw(raw_handle))
}

/// Create a kernel object through an explicitly authorized Factory capability.
pub fn factory_create(
    factory: Handle,
    object_type: gaxera_abi::ObjectType,
) -> Result<Handle, SyscallError> {
    let (status, raw_handle) = unsafe {
        raw_syscall::raw_syscall6_ret2(
            factory.raw(),
            OperationCode::FactoryCreate as u64,
            object_type as u64,
            0,
            0,
            0,
        )
    };
    decode_status(status)?;
    Ok(Handle::from_raw(raw_handle))
}

/// Create a legacy IRQ capability through a Factory. The IRQ line is an
/// explicit argument; callers cannot obtain a hardware interrupt authority
/// from an ambient default.
pub fn factory_create_interrupt(factory: Handle, irq: u8) -> Result<Handle, SyscallError> {
    let (status, raw_handle) = unsafe {
        raw_syscall::raw_syscall6_ret2(
            factory.raw(),
            OperationCode::FactoryCreate as u64,
            gaxera_abi::ObjectType::InterruptObject as u64,
            irq as u64,
            0,
            0,
        )
    };
    decode_status(status)?;
    Ok(Handle::from_raw(raw_handle))
}

/// Create a physically contiguous DMA frame object through a Factory.  The
/// requested byte count is rounded by the kernel to the object's page span.
pub fn factory_create_contiguous_frame(
    factory: Handle,
    size_bytes: u64,
) -> Result<Handle, SyscallError> {
    let (status, raw_handle) = unsafe {
        raw_syscall::raw_syscall6_ret2(
            factory.raw(),
            OperationCode::FactoryCreate as u64,
            gaxera_abi::ObjectType::ContiguousFrame as u64,
            size_bytes,
            0,
            0,
        )
    };
    decode_status(status)?;
    Ok(Handle::from_raw(raw_handle))
}

/// Query the physical base and byte span of a DMA frame capability.  The
/// kernel exposes only the explicitly authorized object and no arbitrary
/// physical-address read primitive.
pub fn contiguous_frame_info(frame: Handle) -> Result<(u64, u64), SyscallError> {
    let (status, physical_base, size_bytes) = unsafe {
        raw_syscall::raw_syscall6_ret3(frame.raw(), OperationCode::Call as u64, 0, 0, 0, 0)
    };
    decode_status(status)?;
    Ok((physical_base, size_bytes))
}

/// Create a child Process through a Factory capability.
///
/// The kernel returns status in `RAX` and the Process handle in `RDX`. The
/// quota arguments are intentionally explicit; no ambient process defaults
/// are applied by this wrapper.
pub fn create_process(
    factory: Handle,
    max_objects: u32,
    max_capabilities: u32,
    max_memory_bytes: u64,
) -> Result<Handle, SyscallError> {
    let (status, raw_handle) = unsafe {
        raw_syscall::raw_syscall6_ret2(
            factory.raw(),
            OperationCode::CreateProcess as u64,
            max_objects as u64,
            max_capabilities as u64,
            max_memory_bytes,
            0,
        )
    };
    decode_status(status)?;
    Ok(Handle::from_raw(raw_handle))
}

/// Invoke a lifecycle operation on a Process capability.
pub fn process_control(
    process: Handle,
    operation: gaxera_abi::ProcessControlOp,
    arg1: u64,
    arg2: u64,
    arg3: u64,
) -> Result<u64, SyscallError> {
    let (status, value) = unsafe {
        raw_syscall::raw_syscall6_ret2(
            process.raw(),
            OperationCode::ProcessControl as u64,
            operation as u64,
            arg1,
            arg2,
            arg3,
        )
    };
    decode_status(status)?;
    Ok(value)
}

/// Query a process and return both its lifecycle state and exit status.
pub fn process_query(process: Handle) -> Result<(u64, u64), SyscallError> {
    let (status, state, exit_status) = unsafe {
        raw_syscall::raw_syscall6_ret3(
            process.raw(),
            OperationCode::ProcessControl as u64,
            gaxera_abi::ProcessControlOp::Query as u64,
            0,
            0,
            0,
        )
    };
    decode_status(status)?;
    Ok((state, exit_status))
}

/// Acquire the child capability space so a supervisor can install delegated
/// capabilities without relying on an implementation-defined slot.
pub fn process_capability_space(process: Handle) -> Result<Handle, SyscallError> {
    let raw = process_control(
        process,
        gaxera_abi::ProcessControlOp::AcquireCapabilitySpace,
        0,
        0,
        0,
    )?;
    Ok(Handle::from_raw(raw))
}

/// Write a bounded diagnostic string through a capability-backed DebugConsole.
pub fn debug_console_write(console: Handle, message: &str) -> Result<(), SyscallError> {
    // The Write syscall has four 64-bit payload registers, so its wire limit
    // is 32 bytes even though inline IPC messages may be larger.
    for chunk in message.as_bytes().chunks(32) {
        let mut words = [0u64; 4];
        for (index, word) in chunk.chunks(8).enumerate() {
            let mut bytes = [0u8; 8];
            bytes[..word.len()].copy_from_slice(word);
            words[index] = u64::from_le_bytes(bytes);
        }
        let status = raw_invoke(
            OperationCode::Write,
            console,
            words[0],
            words[1],
            words[2],
            words[3],
        );
        decode_status(status)?;
    }
    Ok(())
}

/// Map a MemoryObject into an AddressSpace.
///
/// Returns `Ok(Handle)` to the new `Mapping` capability on success, or `Err(SyscallError)` on failure.
pub fn map_memory(
    aspace: Handle,
    memory_object: Handle,
    vaddr: u64,
    rights: gaxera_abi::Rights,
) -> Result<Handle, SyscallError> {
    let (status, new_handle) = unsafe {
        raw_syscall::raw_syscall6_ret2(
            aspace.raw(),
            OperationCode::MapMemory as u64,
            memory_object.raw(),
            vaddr,
            rights.bits() as u64,
            0,
        )
    };
    decode_status(status)?;
    Ok(Handle::from_raw(new_handle))
}

/// Unmap a Mapping capability from an AddressSpace.
pub fn unmap_memory(mapping: Handle) -> Result<(), SyscallError> {
    let (status, _) = unsafe {
        raw_syscall::raw_syscall6_ret2(mapping.raw(), OperationCode::UnmapMemory as u64, 0, 0, 0, 0)
    };
    decode_status(status)?;
    Ok(())
}

/// Execute a generic raw syscall through architecture assembly trampolines.
pub fn raw_invoke(
    opcode: OperationCode,
    handle: Handle,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    _arg4: u64,
) -> u64 {
    // SAFETY: Raw assembly syscall adhering to Gaxera ABI register conventions.
    unsafe { raw_syscall::raw_syscall6(handle.raw(), opcode as u64, arg1, arg2, arg3, _arg4) }
}

/// Yield execution to the kernel scheduler.
pub fn yield_now() -> Result<(), SyscallError> {
    let ret = raw_invoke(OperationCode::YieldProcess, Handle::INVALID, 0, 0, 0, 0);
    if ret == 0 {
        Ok(())
    } else {
        Err(SyscallError::from_code(ret))
    }
}

/// Terminate the current process cleanly with exit code.
pub fn exit(code: u64) -> ! {
    raw_invoke(OperationCode::ExitProcess, Handle::INVALID, code, 0, 0, 0);
    loop {
        // Fallback loop if syscall returns unexpectedly
        let _ = yield_now();
    }
}

/// Delete capability handle slot from process CSpace.
pub fn delete_handle(handle: Handle) -> Result<(), SyscallError> {
    // DeleteHandle has no dispatcher capability. Its target handle is the
    // first syscall argument, which the kernel receives in %rdx.
    let ret = raw_invoke(
        OperationCode::DeleteHandle,
        Handle::INVALID,
        handle.raw(),
        0,
        0,
        0,
    );
    if ret == 0 {
        Ok(())
    } else {
        Err(SyscallError::from_code(ret))
    }
}

/// Perform IPC Call rendezvous to server endpoint.
pub fn ipc_call(endpoint: Handle, msg: &InlineMessage) -> Result<InlineMessage, SyscallError> {
    let mut reply_bytes = [0u8; gaxera_abi::ipc::INLINE_MESSAGE_BYTES];
    let ret = raw_invoke(
        OperationCode::Call,
        endpoint,
        msg.payload().len() as u64,
        msg.payload().as_ptr() as u64,
        reply_bytes.as_mut_ptr() as u64,
        0,
    );
    if ret == 0 {
        InlineMessage::try_new(&reply_bytes).map_err(|_| SyscallError::InvalidArgument)
    } else {
        Err(SyscallError::from_code(ret))
    }
}

/// Perform IPC Reply to woken client thread.
pub fn ipc_reply(caller: Handle, msg: &InlineMessage) -> Result<(), SyscallError> {
    let ret = raw_invoke(
        OperationCode::Reply,
        caller,
        msg.payload().len() as u64,
        msg.payload().as_ptr() as u64,
        0,
        0,
    );
    if ret == 0 {
        Ok(())
    } else {
        Err(SyscallError::from_code(ret))
    }
}

/// Wait on a Notification object, returning pending signal bits.
pub fn wait_notification(notification: Handle) -> Result<u32, SyscallError> {
    let ret = raw_invoke(OperationCode::WaitNotification, notification, 0, 0, 0, 0);
    if (ret as i64) < 0 {
        Err(SyscallError::from_code(ret))
    } else {
        Ok(ret as u32)
    }
}

/// Execute control operation on Interrupt capability object.
pub fn interrupt_control(interrupt: Handle, op: InterruptOp) -> Result<(), SyscallError> {
    interrupt_control_with_arg(interrupt, op, 0)
}

/// Execute control operation with argument on Interrupt capability object.
pub fn interrupt_control_with_arg(
    interrupt: Handle,
    op: InterruptOp,
    arg: u64,
) -> Result<(), SyscallError> {
    let ret = raw_invoke(
        OperationCode::InterruptControl,
        interrupt,
        op as u64,
        arg,
        0,
        0,
    );
    if ret == 0 {
        Ok(())
    } else {
        Err(SyscallError::from_code(ret))
    }
}

/// Wait on a WaitSet for atomic event multiplexing into a caller-provided event buffer.
pub fn waitset_wait(waitset: Handle, events: &mut [WaitSetEvent]) -> Result<usize, SyscallError> {
    if events.is_empty() {
        return Ok(0);
    }
    let ret = raw_invoke(
        OperationCode::WaitSetWait,
        waitset,
        events.as_mut_ptr() as u64,
        events.len() as u64,
        0,
        0,
    );
    if (ret as i64) < 0 {
        Err(SyscallError::from_code(ret))
    } else {
        Ok(ret as usize)
    }
}
