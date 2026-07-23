use gaxera_abi::Handle;

use crate::syscall::delete_handle;

/// Exclusive owned capability slot handle (`!Copy`, `!Clone`).
///
/// Automatically invokes `sys_delete_handle` on `Drop`.
#[derive(Debug)]
pub struct OwnedHandle {
    handle: Handle,
}

impl OwnedHandle {
    /// Creates an `OwnedHandle` assuming exclusive ownership over the capability slot.
    pub fn from_raw_handle(handle: Handle) -> Self {
        Self { handle }
    }

    /// Borrows the underlying `Handle` transiently for syscall parameter passing.
    pub fn as_handle(&self) -> Handle {
        self.handle
    }

    /// Extracts raw `Handle` and transfers ownership out without invoking `Drop`.
    pub fn into_raw_handle(self) -> Handle {
        let h = self.handle;
        core::mem::forget(self);
        h
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if self.handle.is_valid() {
            let _ = delete_handle(self.handle);
        }
    }
}
