#![allow(dead_code)]
use core::arch::asm;
use gaxera_abi::ipc::{InlineMessage, ReplyToken};
use gaxera_abi::{Handle, ObjectType, OperationCode, Rights};

pub unsafe fn sys_invoke(handle: u64, op: u64, arg1: u64, arg2: u64, arg3: u64, arg4: u64) -> u64 {
    let result: u64;
    // SAFETY: We are invoking a kernel system call using the ABI format.
    // The kernel will validate the arguments and return a safe result.
    unsafe {
        asm!(
            "syscall",
            inout("rax") 10u64 => result,
            in("rdi") handle,
            in("rsi") op,
            in("rdx") arg1,
            in("r10") arg2,
            in("r8") arg3,
            in("r9") arg4,
            out("rcx") _,
            out("r11") _,
            options(nostack)
        );
    }
    result
}

// ABI wrappers

pub fn factory_create(factory: Handle, obj_type: ObjectType) -> Result<Handle, ()> {
    // SAFETY: Invoking kernel ABI to create a factory object.
    let result = unsafe { sys_invoke(factory.raw(), 0, obj_type as u64, 0, 0, 0) };
    if result == u64::MAX {
        Err(())
    } else {
        Ok(Handle::from_raw(result))
    }
}

pub fn factory_create_memory(factory: Handle, size: u64) -> Result<Handle, ()> {
    // SAFETY: Invoking kernel ABI to create a memory object.
    let result = unsafe {
        sys_invoke(
            factory.raw(),
            0,
            ObjectType::MemoryObject as u64,
            size,
            0,
            0,
        )
    };
    if result == u64::MAX {
        Err(())
    } else {
        Ok(Handle::from_raw(result))
    }
}

pub fn thread_configure(
    thread: Handle,
    rip: u64,
    rsp: u64,
    aspace: Handle,
    cspace: Handle,
) -> Result<(), ()> {
    // SAFETY: Invoking kernel ABI to configure thread.
    let result = unsafe {
        sys_invoke(
            thread.raw(),
            OperationCode::ConfigureThread as u64,
            rip,
            rsp,
            aspace.raw(),
            cspace.raw(),
        )
    };
    if result == u64::MAX { Err(()) } else { Ok(()) }
}

pub fn derive_capability(
    source_handle: Handle,
    target_cspace: Handle,
    requested_rights: Rights,
) -> Result<Handle, ()> {
    // SAFETY: Invoking kernel ABI to derive capability.
    let result = unsafe {
        sys_invoke(
            source_handle.raw(),
            OperationCode::Derive as u64,
            target_cspace.raw(),
            requested_rights.bits() as u64,
            0,
            0,
        )
    };
    if result == u64::MAX {
        Err(())
    } else {
        Ok(Handle::from_raw(result))
    }
}

pub fn map_memory(
    aspace: Handle,
    memory_object: Handle,
    vaddr: u64,
    rights: Rights,
) -> Result<(), ()> {
    // SAFETY: Invoking kernel ABI to map memory.
    let res = unsafe {
        sys_invoke(
            aspace.raw(),
            OperationCode::MapMemory as u64,
            memory_object.raw(),
            vaddr,
            rights.bits() as u64,
            0,
        )
    };
    if res == 0 { Ok(()) } else { Err(()) }
}

pub fn endpoint_call(endpoint: Handle, message: &InlineMessage) -> Result<(), ()> {
    let payload = message.payload();
    let mut args = [0u64; 4];
    if payload.len() >= 8 {
        args[0] = u64::from_le_bytes(payload[0..8].try_into().unwrap());
    }
    if payload.len() >= 16 {
        args[1] = u64::from_le_bytes(payload[8..16].try_into().unwrap());
    }
    if payload.len() >= 24 {
        args[2] = u64::from_le_bytes(payload[16..24].try_into().unwrap());
    }
    if payload.len() >= 32 {
        args[3] = u64::from_le_bytes(payload[24..32].try_into().unwrap());
    }

    // SAFETY: Invoking kernel ABI for endpoint call.
    let res = unsafe {
        sys_invoke(
            endpoint.raw(),
            OperationCode::Call as u64,
            args[0],
            args[1],
            args[2],
            args[3],
        )
    };
    if res == 0 { Ok(()) } else { Err(()) }
}

pub struct ReceivedCall {
    pub reply_token: ReplyToken,
    pub caller: Handle,
    pub message: InlineMessage,
}

pub fn endpoint_receive(endpoint: Handle) -> Result<ReceivedCall, ()> {
    let mut result_token: u64;
    let mut result_caller: u64;
    let mut arg1: u64;
    let mut arg2: u64;
    let mut arg3: u64;
    let mut arg4: u64;

    // SAFETY: Manually assembling syscall ABI for Receive to capture output registers.
    unsafe {
        asm!(
            "syscall",
            in("rax") 10,
            in("rdi") endpoint.raw(),
            in("rsi") OperationCode::Receive as u64,
            in("rdx") 0,
            in("r10") 0,
            in("r8") 0,
            in("r9") 0,
            lateout("rax") _, // syscall result
            lateout("rdi") result_token,
            lateout("rsi") result_caller,
            lateout("rdx") arg1,
            lateout("r10") arg2,
            lateout("r8") arg3,
            lateout("r9") arg4,
            options(nostack)
        );
    }

    let mut payload = [0u8; 32];
    payload[0..8].copy_from_slice(&arg1.to_le_bytes());
    payload[8..16].copy_from_slice(&arg2.to_le_bytes());
    payload[16..24].copy_from_slice(&arg3.to_le_bytes());
    payload[24..32].copy_from_slice(&arg4.to_le_bytes());

    let message = InlineMessage::try_new(&payload).unwrap();
    Ok(ReceivedCall {
        reply_token: ReplyToken::from_raw(result_token),
        caller: Handle::from_raw(result_caller),
        message,
    })
}

pub fn endpoint_reply(
    endpoint: Handle,
    _token: ReplyToken,
    message: &InlineMessage,
) -> Result<(), ()> {
    let payload = message.payload();
    let mut args = [0u64; 4];
    if payload.len() >= 8 {
        args[0] = u64::from_le_bytes(payload[0..8].try_into().unwrap());
    }
    if payload.len() >= 16 {
        args[1] = u64::from_le_bytes(payload[8..16].try_into().unwrap());
    }
    if payload.len() >= 24 {
        args[2] = u64::from_le_bytes(payload[16..24].try_into().unwrap());
    }
    if payload.len() >= 32 {
        args[3] = u64::from_le_bytes(payload[24..32].try_into().unwrap());
    }

    // SAFETY: Invoking kernel ABI for endpoint reply.
    let res = unsafe {
        sys_invoke(
            endpoint.raw(),
            OperationCode::Reply as u64,
            args[0],
            args[1],
            args[2],
            args[3],
        )
    };
    if res == 0 { Ok(()) } else { Err(()) }
}

pub fn debug_console_write(console: Handle, message: &str) -> Result<(), ()> {
    let bytes = message.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let mut payload = [0u8; 32];
        let chunk_size = core::cmp::min(32, bytes.len() - offset);
        payload[..chunk_size].copy_from_slice(&bytes[offset..offset + chunk_size]);

        let mut args = [0u64; 4];
        args[0] = u64::from_le_bytes(payload[0..8].try_into().unwrap());
        args[1] = u64::from_le_bytes(payload[8..16].try_into().unwrap());
        args[2] = u64::from_le_bytes(payload[16..24].try_into().unwrap());
        args[3] = u64::from_le_bytes(payload[24..32].try_into().unwrap());

        // SAFETY: Invoking kernel ABI to write to console.
        let res = unsafe {
            sys_invoke(
                console.raw(),
                OperationCode::Write as u64,
                args[0],
                args[1],
                args[2],
                args[3],
            )
        };
        if res != 0 {
            return Err(());
        }
        offset += chunk_size;
    }
    Ok(())
}

pub fn thread_status(thread: Handle) -> Result<u64, ()> {
    // SAFETY: Invoking kernel ABI to check thread status.
    let res = unsafe { sys_invoke(thread.raw(), OperationCode::ThreadStatus as u64, 0, 0, 0, 0) };
    if res == u64::MAX { Err(()) } else { Ok(res) }
}
