#![no_std]
#![feature(abi_x86_interrupt)]
#![feature(never_type)]
#![allow(clippy::undocumented_unsafe_blocks)]

extern crate alloc;

pub mod arch;
pub mod framebuffer;
pub mod init;
pub mod memory;
pub mod serial;
