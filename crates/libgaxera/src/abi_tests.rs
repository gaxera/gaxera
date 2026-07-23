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
