use gaxera_abi::{Handle, InterruptOp};

use crate::object::handle::OwnedHandle;
use crate::syscall::{interrupt_control, SyscallError};

/// Type-safe Interrupt handle wrapping an `OwnedHandle`.
#[derive(Debug)]
pub struct InterruptHandle {
    inner: OwnedHandle,
}

impl InterruptHandle {
    pub fn new(owned: OwnedHandle) -> Self {
        Self { inner: owned }
    }

    pub fn from_raw(handle: Handle) -> Self {
        Self {
            inner: OwnedHandle::from_raw_handle(handle),
        }
    }

    pub fn as_handle(&self) -> Handle {
        self.inner.as_handle()
    }

    pub fn mask(&self) -> Result<(), SyscallError> {
        interrupt_control(self.inner.as_handle(), InterruptOp::Mask)
    }

    pub fn unmask(&self) -> Result<(), SyscallError> {
        interrupt_control(self.inner.as_handle(), InterruptOp::Unmask)
    }

    pub fn ack(&self) -> Result<(), SyscallError> {
        interrupt_control(self.inner.as_handle(), InterruptOp::Ack)
    }
}
