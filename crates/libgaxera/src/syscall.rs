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
    InternalError,
    Unknown(u64),
}

impl SyscallError {
    pub fn from_code(code: u64) -> Self {
        match code {
            1 => Self::InvalidHandle,
            2 => Self::RightsDenied,
            3 => Self::InvalidArgument,
            4 => Self::ResourceExhausted,
            5 => Self::TimedOut,
            u64::MAX => Self::InternalError,
            other => Self::Unknown(other),
        }
    }
}

/// Execute a generic raw syscall through architecture assembly trampolines.
pub fn raw_invoke(
    opcode: OperationCode,
    handle: Handle,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
) -> u64 {
    // SAFETY: Raw assembly syscall adhering to Gaxera ABI register conventions.
    unsafe { raw_syscall::raw_syscall6(opcode as u64, handle.raw(), arg1, arg2, arg3, arg4) }
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
    let ret = raw_invoke(OperationCode::DeleteHandle, handle, 0, 0, 0, 0);
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
    let ret = raw_invoke(
        OperationCode::InterruptControl,
        interrupt,
        op as u64,
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
