/// Standard Block Service IPC Operation Codes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum BlockOp {
    ReadSectors = 1,
    WriteSectors = 2,
    Flush = 3,
    QueryCapacity = 4,
}

/// Standardized Transport-Independent Block Error Codes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u32)]
pub enum BlockError {
    Success = 0,
    IoError = 1,
    UnsupportedOperation = 2,
    DeviceFailure = 3,
    InvalidParameter = 4,
}
