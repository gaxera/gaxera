/// Standard Network Service IPC Operation Codes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum NetOp {
    SendPacket = 1,
    ReceivePacket = 2,
    ReleaseBuffer = 3,
    GetMacAddress = 4,
    QueryStatus = 5,
}

/// Standardized Transport-Independent Network Error Codes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum NetError {
    Success = 0,
    BufferExhausted = 1,
    InvalidBufferId = 2,
    DeviceFailure = 3,
    LinkDown = 4,
    PermissionDenied = 5,
}
