#[cfg(feature = "qemu-test")]
use x86_64::instructions::port::PortWriteOnly;

#[cfg(feature = "qemu-test")]
use crate::serial;

#[cfg(feature = "qemu-test")]
const ISA_DEBUG_EXIT_PORT: u16 = 0xF4;
#[cfg(feature = "qemu-test")]
const SUCCESS_CODE: u32 = 0x10;
#[cfg(feature = "qemu-test")]
const FAILURE_CODE: u32 = 0x11;

/// End a feature-gated QEMU integration test with a deterministic success code.
///
/// # Safety
/// This writes to QEMU's `isa-debug-exit` device. It must only execute in a
/// test image launched with that device; production images must never depend on
/// emulator-specific I/O to make progress.
#[cfg(feature = "qemu-test")]
pub unsafe fn exit_success() -> ! {
    // SAFETY: The caller established that this is a QEMU test image with the
    // matching device attached.
    unsafe { exit(SUCCESS_CODE) }
}

/// End a feature-gated QEMU integration test with a deterministic failure code.
///
/// # Safety
/// This writes to QEMU's `isa-debug-exit` device. It must only execute in a
/// test image launched with that device.
#[cfg(feature = "qemu-test")]
pub unsafe fn exit_failure() -> ! {
    // SAFETY: The caller established that this is a QEMU test image with the
    // matching device attached.
    unsafe { exit(FAILURE_CODE) }
}

#[cfg(feature = "qemu-test")]
unsafe fn exit(code: u32) -> ! {
    let mut port = PortWriteOnly::<u32>::new(ISA_DEBUG_EXIT_PORT);
    // SAFETY: `xtask` attaches `isa-debug-exit` at this port for every
    // qemu-test image. QEMU terminates the guest process immediately.
    unsafe {
        port.write(code);
    }
    serial::halt();
}
