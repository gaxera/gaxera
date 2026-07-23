use gaxera_abi::Handle;

use crate::object::handle::OwnedHandle;
use crate::syscall::{wait_notification, SyscallError};

/// Type-safe Notification handle wrapping an `OwnedHandle`.
#[derive(Debug)]
pub struct NotificationHandle {
    inner: OwnedHandle,
}

impl NotificationHandle {
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

    pub fn wait(&self) -> Result<u32, SyscallError> {
        wait_notification(self.inner.as_handle())
    }
}
