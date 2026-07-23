#[allow(unused_imports)]
use crate::syscall::exit;

/// Standard ring-3 process entrypoint `_start`.
///
/// # Safety
/// Executed by the kernel on ring-3 process startup.
#[cfg(all(feature = "entry", not(test)))]
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    extern "Rust" {
        fn main() -> i32;
    }

    let status = unsafe { main() };
    exit(status as u64);
}
