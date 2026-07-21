#![no_std]
#![cfg_attr(not(test), no_main)]

#[cfg(not(test))]
use core::arch::asm;
#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[inline(always)]
unsafe fn sys_invoke(rdi: u64, rsi: u64, rdx: u64, r10: u64, r8: u64, r9: u64) -> u64 {
    let result: u64;
    // SAFETY: Syscall ABI is defined and respects register constraints.
    unsafe {
        asm!(
            "syscall",
            inout("rax") 10u64 => result,
            in("rdi") rdi,
            in("rsi") rsi,
            in("rdx") rdx,
            in("r10") r10,
            in("r8") r8,
            in("r9") r9,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    result
}

#[cfg(not(test))]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let endpoint = gaxera_abi::svc::ENDPOINT_RAMFS;
    let ramfs_base = gaxera_abi::svc::RAMFS_BASE as *const u8;
    let max_size = gaxera_abi::svc::RAMFS_MAX_SIZE;

    // Check magic
    // SAFETY: Bootloader maps this range securely.
    let magic = unsafe { core::slice::from_raw_parts(ramfs_base, 8) };
    if magic != b"GAXFS\0\0\0" {
        panic!("Invalid ramfs magic");
    }

    loop {
        // Receive request
        let mut msg = [0u64; 4];
        let token =
            // SAFETY: Valid endpoint provided by init.
            unsafe {
                let mut _status: u64;
            let mut res: u64;
            asm!(
                "syscall",
                inout("rax") 10u64 => _status,
                in("rdi") endpoint,
                in("rsi") 3, // OperationCode::Receive
                in("rdx") 0,
                in("r10") 0,
                in("r8") 0,
                in("r9") 0,
                lateout("rdi") res,
                lateout("rcx") _,
                lateout("r11") _,
                lateout("rsi") _,
                lateout("rdx") msg[0],
                lateout("r10") msg[1],
                lateout("r8") msg[2],
                lateout("r9") msg[3],
                options(nostack)
            );
            res
        };

        let file_id_req = msg[0] as u32;
        let mut reply = [0u64; 4];
        parse_gaxfs(ramfs_base, max_size, file_id_req, &mut reply);

        // Reply
        // SAFETY: IPC reply is permitted and token is valid.
        unsafe {
            sys_invoke(
                token, 4, // OperationCode::Reply
                reply[0], reply[1], reply[2], reply[3],
            );
        }
    }
}

/// Parses the GAXFS archive to find a file and populate the reply payload.
fn parse_gaxfs(ramfs_base: *const u8, max_size: usize, file_id_req: u32, reply: &mut [u64; 4]) {
    if max_size < 16 {
        reply[0] = gaxera_abi::svc::STATUS_NOT_FOUND;
        return;
    }

    // SAFETY: Pointer is within mapped ramfs bounds as checked by max_size.
    let num_files = unsafe { core::ptr::read_unaligned(ramfs_base.add(12) as *const u32) };

    // Validate that the number of files doesn't push the headers out of bounds
    // Each entry is 44 bytes, header is 16 bytes.
    if let Some(total_header_size) = (num_files as usize)
        .checked_mul(44)
        .and_then(|v| v.checked_add(16))
    {
        if total_header_size > max_size {
            reply[0] = gaxera_abi::svc::STATUS_NOT_FOUND;
            return;
        }
    } else {
        reply[0] = gaxera_abi::svc::STATUS_NOT_FOUND;
        return;
    }

    let mut found = false;
    let mut file_offset = 0;
    let mut file_size = 0;

    for i in 0..num_files {
        // SAFETY: Bounds checked against max_size above.
        let entry_ptr = unsafe { ramfs_base.add(16 + (i as usize) * 44) };
        // SAFETY: Pointer is within bounds.
        let fid = unsafe { core::ptr::read_unaligned(entry_ptr as *const u32) };
        if fid == file_id_req {
            found = true;
            // SAFETY: Valid struct fields within entry bounds.
            file_offset = unsafe { core::ptr::read_unaligned(entry_ptr.add(36) as *const u32) };
            // SAFETY: Valid struct fields within entry bounds.
            file_size = unsafe { core::ptr::read_unaligned(entry_ptr.add(40) as *const u32) };
            break;
        }
    }

    if found {
        // Validate file_offset and file_size against max_size
        let is_valid_range = (file_offset as usize)
            .checked_add(file_size as usize)
            .is_some_and(|end| end <= max_size);
        if is_valid_range {
            // SAFETY: The offset and size are fully validated against max_size.
            let slice = unsafe {
                core::slice::from_raw_parts(
                    ramfs_base.add(file_offset as usize),
                    core::cmp::min(file_size as usize, 24),
                )
            };
            let mut buf = [0u8; 24];
            buf[..slice.len()].copy_from_slice(slice);
            reply[0] = gaxera_abi::svc::STATUS_OK;
            reply[1] = u64::from_le_bytes(buf[0..8].try_into().unwrap());
            reply[2] = u64::from_le_bytes(buf[8..16].try_into().unwrap());
            reply[3] = u64::from_le_bytes(buf[16..24].try_into().unwrap());
            return;
        }
    }

    reply[0] = gaxera_abi::svc::STATUS_NOT_FOUND;
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        // SAFETY: Halting execution safely.
        unsafe { asm!("pause") }
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn parse_gaxfs_never_panics(
            buf in proptest::collection::vec(any::<u8>(), 0..10000),
            req_id in any::<u32>(),
            max_size in 0usize..20000usize
        ) {
            let mut reply = [0u64; 4];
            let buf_ptr = if buf.is_empty() {
                core::ptr::null()
            } else {
                buf.as_ptr()
            };

            let safe_max_size = core::cmp::min(max_size, buf.len());
            if safe_max_size > 0 && !buf_ptr.is_null() {
                parse_gaxfs(buf_ptr, safe_max_size, req_id, &mut reply);
            }
        }
    }
}
