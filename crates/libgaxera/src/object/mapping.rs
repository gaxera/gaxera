use gaxera_abi::Handle;

use crate::object::handle::OwnedHandle;

/// Type-safe Mapping handle wrapping an `OwnedHandle`.
#[derive(Debug)]
pub struct MappingHandle {
    inner: OwnedHandle,
}

impl MappingHandle {
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
}
