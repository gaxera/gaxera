use gaxera_abi::ipc::InlineMessage;
use gaxera_abi::Handle;

use crate::object::handle::OwnedHandle;
use crate::syscall::{ipc_call, ipc_reply, SyscallError};

/// Type-safe IPC Endpoint handle wrapping an `OwnedHandle`.
#[derive(Debug)]
pub struct EndpointHandle {
    inner: OwnedHandle,
}

impl EndpointHandle {
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

    pub fn call(&self, msg: &InlineMessage) -> Result<InlineMessage, SyscallError> {
        ipc_call(self.inner.as_handle(), msg)
    }

    pub fn reply(&self, msg: &InlineMessage) -> Result<(), SyscallError> {
        ipc_reply(self.inner.as_handle(), msg)
    }
}
