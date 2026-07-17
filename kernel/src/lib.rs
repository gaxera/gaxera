#![no_std]
#![feature(abi_x86_interrupt)]

extern crate alloc;

pub mod arch;
pub mod framebuffer;
pub mod memory;
pub mod serial;
