#![no_std]
#![cfg_attr(not(test), no_main)]

extern crate alloc;

use alloc::vec::Vec;
use core::alloc::{GlobalAlloc, Layout};
#[cfg(not(test))]
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
    let tcp_engine = net_stack_server::tcp::TcpTransportEngine::new();
    let udp_engine = net_stack_server::udp::UdpTransportEngine::new();
    let router = net_stack_server::ip_router::IpRouter::new();
    let _ = (&tcp_engine, &udp_engine, &router);

    let mut tx_queue = Vec::new();

    loop {
        let retransmits = tcp_engine.poll_timer_ticks();
        if !retransmits.is_empty() {
            tcp_engine.build_retransmit_frames(&retransmits, &mut tx_queue);
            tx_queue.clear();
        }
        // SAFETY: Pausing CPU execution in active Ring-3 IPC loop.
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
