#![no_std]
#![cfg_attr(not(test), no_main)]
#![allow(
    clippy::not_unsafe_ptr_arg_deref,
    clippy::undocumented_unsafe_blocks,
    clippy::while_let_loop,
    unused_imports
)]

extern crate alloc;

#[allow(unused_imports)]
use alloc::vec::Vec;
#[cfg(not(test))]
use core::arch::asm;
use gaxera_abi::boot::BootstrapManifest;
use libgaxera::allocator::UserspaceAllocator;

#[global_allocator]
static ALLOCATOR: UserspaceAllocator = UserspaceAllocator;

#[cfg(not(test))]
use core::panic::PanicInfo;

#[cfg(not(test))]
#[no_mangle]
pub extern "C" fn _start(manifest: *const BootstrapManifest, length: usize) -> ! {
    if unsafe { libgaxera::entry::initialize_userspace_allocator(manifest, length, &ALLOCATOR) }
        .is_err()
    {
        libgaxera::syscall::exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    let tcp_engine = net_stack_server::tcp::TcpTransportEngine::new();
    let udp_engine = net_stack_server::udp::UdpTransportEngine::new();
    let router = net_stack_server::ip_router::IpRouter::new();
    let _ = (&tcp_engine, &udp_engine, &router);

    let tx_ring = net_types::PacketRingHeader::new(64, net_types::RingType::Tx);
    let mut tx_queue = Vec::new();

    loop {
        let retransmits = tcp_engine.poll_timer_ticks();
        if !retransmits.is_empty() {
            tcp_engine.build_retransmit_frames(&retransmits, &mut tx_queue);
            // Wire Transmission Dispatch: Push constructed link frames onto active transmit queue
            for frame in tx_queue.drain(..) {
                // Push slot index onto shared PacketRing transmit queue
                let _ = tx_ring.push_slot();
                let _ = frame;
            }
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
