use core::mem::{align_of, size_of};
use gaxera_abi::ipc::{InlineMessage, TransferDescriptor};
use gaxera_abi::{Handle, OperationCode, WaitSetEvent};

#[test]
fn abi_layout_and_size_invariants() {
    assert_eq!(size_of::<Handle>(), size_of::<u64>());
    assert_eq!(align_of::<Handle>(), align_of::<u64>());

    assert_eq!(size_of::<WaitSetEvent>(), 16);
    assert_eq!(align_of::<WaitSetEvent>(), 8);

    assert_eq!(size_of::<TransferDescriptor>(), 16);
    assert_eq!(align_of::<TransferDescriptor>(), 8);

    assert!(size_of::<InlineMessage>() > 0);
}

#[test]
fn operation_code_coverage_assertion() {
    let _ = OperationCode::YieldProcess as u64;
    let _ = OperationCode::Call as u64;
    let _ = OperationCode::Reply as u64;
    let _ = OperationCode::WaitSetWait as u64;
    let _ = OperationCode::WaitNotification as u64;
    let _ = OperationCode::InterruptControl as u64;
    let _ = OperationCode::MapMemory as u64;
    let _ = OperationCode::UnmapMemory as u64;
}

#[test]
fn status_code_encoding_and_decoding_invariants() {
    use crate::syscall::SyscallError;
    use gaxera_abi::status;

    // Verify exact numerical ABI values
    assert_eq!(status::SUCCESS, 0);
    assert_eq!(status::INVALID_HANDLE, 1);
    assert_eq!(status::RIGHTS_DENIED, 2);
    assert_eq!(status::INVALID_ARGUMENT, 3);
    assert_eq!(status::RESOURCE_EXHAUSTED, 4);
    assert_eq!(status::TIMED_OUT, 5);
    assert_eq!(status::MAPPING_COLLISION, 6);
    assert_eq!(status::OBJECT_LIMIT, 7);
    assert_eq!(status::CAPABILITY_LIMIT, 8);
    assert_eq!(status::MEMORY_LIMIT, 9);
    assert_eq!(status::INTERNAL_ERROR, u64::MAX);

    // Verify canonical translation point in SyscallError::from_code
    assert_eq!(SyscallError::from_code(1), SyscallError::InvalidHandle);
    assert_eq!(SyscallError::from_code(2), SyscallError::RightsDenied);
    assert_eq!(SyscallError::from_code(3), SyscallError::InvalidArgument);
    assert_eq!(SyscallError::from_code(4), SyscallError::ResourceExhausted);
    assert_eq!(SyscallError::from_code(5), SyscallError::TimedOut);
    assert_eq!(SyscallError::from_code(6), SyscallError::MappingCollision);
    assert_eq!(SyscallError::from_code(7), SyscallError::ObjectLimit);
    assert_eq!(SyscallError::from_code(8), SyscallError::CapabilityLimit);
    assert_eq!(SyscallError::from_code(9), SyscallError::MemoryLimit);
    assert_eq!(
        SyscallError::from_code(u64::MAX),
        SyscallError::InternalError
    );

    // Verify unknown status code preservation
    assert_eq!(
        SyscallError::from_code(0x12345),
        SyscallError::Unknown(0x12345)
    );
    assert_eq!(SyscallError::Unknown(0x12345).to_code(), 0x12345);

    // Verify round-trip encoding
    let error_variants = [
        SyscallError::InvalidHandle,
        SyscallError::RightsDenied,
        SyscallError::InvalidArgument,
        SyscallError::ResourceExhausted,
        SyscallError::TimedOut,
        SyscallError::MappingCollision,
        SyscallError::ObjectLimit,
        SyscallError::CapabilityLimit,
        SyscallError::MemoryLimit,
        SyscallError::InternalError,
    ];
    for err in error_variants {
        assert_eq!(SyscallError::from_code(err.to_code()), err);
    }
}
