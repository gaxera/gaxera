use gaxera_abi::{Handle, WaitSetEvent};

use crate::object::handle::OwnedHandle;
use crate::syscall::{waitset_wait, SyscallError};

/// Type-safe WaitSet handle wrapping an `OwnedHandle`.
#[derive(Debug)]
pub struct WaitSetHandle {
    inner: OwnedHandle,
}

impl WaitSetHandle {
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

    pub fn wait(&self, events: &mut [WaitSetEvent]) -> Result<usize, SyscallError> {
        waitset_wait(self.inner.as_handle(), events)
    }
}
