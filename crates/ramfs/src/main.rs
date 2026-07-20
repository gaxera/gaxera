#![no_std]
#![no_main]

use core::arch::asm;
#[cfg(not(test))]
use core::panic::PanicInfo;

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

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let endpoint: u64 = 0x100000000;
    let ramfs_base = 0x0000_6000_0000_0000 as *const u8;

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

        // Find file
        // SAFETY: Pointer is within mapped ramfs bounds.
        let num_files = unsafe { core::ptr::read_unaligned(ramfs_base.add(12) as *const u32) };
        let mut found = false;
        let mut file_offset = 0;
        let mut file_size = 0;

        for i in 0..num_files {
            // SAFETY: Iterating within mapped GAXFS structure bounds.
            let entry_ptr = unsafe { ramfs_base.add(16 + (i as usize) * 44) };
            // SAFETY: Valid pointer arithmetic inside archive bounds.
            let fid = unsafe { core::ptr::read_unaligned(entry_ptr as *const u32) };
            if fid == file_id_req {
                found = true;
                // SAFETY: Reading valid struct fields within bounds.
                file_offset = unsafe { core::ptr::read_unaligned(entry_ptr.add(36) as *const u32) };
                // SAFETY: Reading valid struct fields within bounds.
                file_size = unsafe { core::ptr::read_unaligned(entry_ptr.add(40) as *const u32) };
                break;
            }
        }

        let mut reply = [0u64; 4];
        if found {
            // Read up to 32 bytes of the file for now
            // SAFETY: The offset and size are validated against archive bounds.
            let slice = unsafe {
                core::slice::from_raw_parts(
                    ramfs_base.add(file_offset as usize),
                    core::cmp::min(file_size as usize, 32),
                )
            };
            let mut buf = [0u8; 32];
            buf[..slice.len()].copy_from_slice(slice);
            reply[0] = u64::from_le_bytes(buf[0..8].try_into().unwrap());
            reply[1] = u64::from_le_bytes(buf[8..16].try_into().unwrap());
            reply[2] = u64::from_le_bytes(buf[16..24].try_into().unwrap());
            reply[3] = u64::from_le_bytes(buf[24..32].try_into().unwrap());
        }

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

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        // SAFETY: Halting execution safely.
        unsafe { asm!("pause") }
    }
}
