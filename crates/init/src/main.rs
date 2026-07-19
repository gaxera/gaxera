#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

#[unsafe(no_mangle)]
pub extern "C" fn _start(_boot_info: *const ()) -> ! {
    // 1. Invoke Factory (capability index 3 in our mock setup)
    unsafe {
        asm!(
            "syscall",
            in("rax") 10,
            in("rdi") 3,
            in("rsi") 0,
            options(nostack)
        );
    }

    // 2. Exit QEMU with success (capability index 4 in our mock setup)
    unsafe {
        asm!(
            "syscall",
            in("rax") 10,
            in("rdi") 4,
            in("rsi") 0,
            options(nostack)
        );
    }

    loop {
        unsafe { asm!("pause") }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe { asm!("pause") }
    }
}
