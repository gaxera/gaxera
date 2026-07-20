#![no_std]
#![feature(abi_x86_interrupt)]
#![feature(never_type)]

extern crate alloc;

pub mod arch;
pub mod framebuffer;
pub mod global;
pub mod init;
pub mod memory;
pub mod serial;
