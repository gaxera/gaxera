#![no_std]
#![cfg_attr(not(test), no_main)]

use core::alloc::{GlobalAlloc, Layout};
use core::arch::asm;

struct DummyAllocator;
// SAFETY: Dummy allocator fulfilling no_std global_allocator requirement.
unsafe impl GlobalAlloc for DummyAllocator {
    unsafe fn alloc(&self, _layout: Layout) -> *mut u8 {
        core::ptr::null_mut()
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: DummyAllocator = DummyAllocator;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    let driver = virtio_gpu_server::gpu::VirtioGpuDriver::new(1024, 768);
    let _ = &driver;

    loop {
        // SAFETY: Active Ring-3 IPC event loop waiting on VirtIO-GPU scanout & flush requests.
        unsafe { asm!("pause") }
    }
}

#[cfg(not(test))]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        // SAFETY: Halting execution.
        unsafe { asm!("pause") }
    }
}
