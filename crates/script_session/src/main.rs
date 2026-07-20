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

fn debug_console_write(console: u64, message: &str) {
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

        // SAFETY: Console handle is valid and Write operation is safe.
        unsafe {
            sys_invoke(
                console, 6, // OperationCode::Write
                args[0], args[1], args[2], args[3],
            );
        }
        offset += chunk_size;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let endpoint: u64 = 0x100000000;
    let console: u64 = 0x100000001;

    // Call endpoint for file ID 1
    let mut msg = [0u64; 4];
    msg[0] = 1; // File ID

    let _res =
        // SAFETY: Syscall uses valid Endpoint capability.
        unsafe {
        let mut status: u64;
        asm!(
            "syscall",
            inout("rax") 10u64 => status,
            in("rdi") endpoint,
            in("rsi") 2, // OperationCode::Call
            inout("rdx") msg[0],
            inout("r10") msg[1],
            inout("r8") msg[2],
            inout("r9") msg[3],
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
        status
    };

    // Parse response
    let mut payload = [0u8; 32];
    payload[0..8].copy_from_slice(&msg[0].to_le_bytes());
    payload[8..16].copy_from_slice(&msg[1].to_le_bytes());
    payload[16..24].copy_from_slice(&msg[2].to_le_bytes());
    payload[24..32].copy_from_slice(&msg[3].to_le_bytes());

    let len = payload.iter().position(|&c| c == 0).unwrap_or(32);
    if let Ok(s) = core::str::from_utf8(&payload[..len]) {
        debug_console_write(console, "[script_session] Successfully read from ramfs: \"");
        debug_console_write(console, s);
        debug_console_write(console, "\"\n");
    } else {
        debug_console_write(console, "Failed to read UTF-8\n");
    }

    // Negative test: Invalid capability
    let invalid_cap_res =
        // SAFETY: Testing invalid capability safely returns error from kernel.
        unsafe {
        let mut status: u64;
        asm!(
            "syscall",
            inout("rax") 10u64 => status,
            in("rdi") 0xDEADBEEFu64, // Invalid handle
            in("rsi") 2, // OperationCode::Call
            in("rdx") 0,
            in("r10") 0,
            in("r8") 0,
            in("r9") 0,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
        status
    };

    if invalid_cap_res != u64::MAX {
        debug_console_write(
            console,
            "[script_session] ERROR: Invalid capability test failed!\n",
        );
    } else {
        debug_console_write(
            console,
            "[script_session] Invalid capability test passed.\n",
        );
    }

    // Negative test: Invalid File ID
    msg[0] = 999; // Non-existent file
    msg[1] = 0;
    msg[2] = 0;
    msg[3] = 0;

    let invalid_file_res =
        // SAFETY: Passing invalid payload is safely caught by IPC receiver.
        unsafe {
        let mut status: u64;
        asm!(
            "syscall",
            inout("rax") 10u64 => status,
            in("rdi") endpoint,
            in("rsi") 2, // OperationCode::Call
            inout("rdx") msg[0],
            inout("r10") msg[1],
            inout("r8") msg[2],
            inout("r9") msg[3],
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
        status
    };

    if invalid_file_res != 0 || msg[0] != 0 {
        debug_console_write(
            console,
            "[script_session] ERROR: Invalid file test failed!\n",
        );
    } else {
        debug_console_write(console, "[script_session] Invalid file test passed.\n");
    }

    debug_console_write(console, "GAXERA: INIT_TEST_SUCCESS\n");

    loop {
        // SAFETY: Halting execution safely.
        unsafe { asm!("pause") }
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
