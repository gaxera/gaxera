#![no_std]
#![no_main]

use core::panic::PanicInfo;

// SAFETY: _start is the sole kernel entry point; no other
// global symbol in this binary can collide with it.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_: &PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
