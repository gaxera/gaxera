use core::arch::asm;

use x86_64::registers::control::{Cr2, Cr3};

use crate::arch::x86_64::{descriptors, entry};
use crate::println;

const MAX_BACKTRACE_FRAMES: usize = 16;
const FRAME_SIZE: u64 = 2 * core::mem::size_of::<u64>() as u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StackBounds {
    start: u64,
    end: u64,
}

impl StackBounds {
    fn contains_frame_pointer(self, frame_pointer: u64) -> bool {
        frame_pointer.is_multiple_of(core::mem::align_of::<u64>() as u64)
            && frame_pointer >= self.start
            && frame_pointer
                .checked_add(FRAME_SIZE)
                .is_some_and(|frame_end| frame_end <= self.end)
    }
}

#[derive(Clone, Copy)]
struct RegisterSnapshot {
    stack_pointer: u64,
    frame_pointer: u64,
    flags: u64,
    fault_address: u64,
    page_table_root: u64,
}

/// Emit bounded, allocation-free diagnostic state from the panic path.
///
/// Frame pointers are forced for every kernel profile in `.cargo/config.toml`.
/// The walker follows only a monotonically increasing chain within the active
/// bootstrap or double-fault stack and stops at `MAX_BACKTRACE_FRAMES`. The
/// result is deliberately raw instruction addresses rather than symbol names:
/// the freestanding kernel discards unwind and debug metadata from its runtime
/// image, and a partial symbolizer would create an unreliable crash path.
pub fn emit_panic_telemetry() {
    let snapshot = capture_registers();
    println!("GAXERA: PANIC_DIAGNOSTICS_BEGIN");
    println!(
        "GAXERA: PANIC_CPU_STATE rsp={:#018x} rbp={:#018x} rflags={:#018x} cr2={:#018x} cr3={:#018x}",
        snapshot.stack_pointer,
        snapshot.frame_pointer,
        snapshot.flags,
        snapshot.fault_address,
        snapshot.page_table_root,
    );
    let frames = print_backtrace(snapshot.frame_pointer);
    println!("GAXERA: PANIC_DIAGNOSTICS_COMPLETE frames={frames}");
}

fn capture_registers() -> RegisterSnapshot {
    let stack_pointer: u64;
    let frame_pointer: u64;
    let flags: u64;
    // SAFETY: this snapshots architectural state without changing it. The
    // balanced push/pop preserves RSP, and `force-frame-pointers=yes` makes
    // RBP a reliable chain anchor for the bounded walker below.
    unsafe {
        asm!(
            "mov {stack_pointer}, rsp",
            "mov {frame_pointer}, rbp",
            "pushfq",
            "pop {flags}",
            stack_pointer = out(reg) stack_pointer,
            frame_pointer = out(reg) frame_pointer,
            flags = out(reg) flags,
            options(preserves_flags),
        );
    }
    let (page_table_root, _) = Cr3::read();
    RegisterSnapshot {
        stack_pointer,
        frame_pointer,
        flags,
        fault_address: Cr2::read_raw(),
        page_table_root: page_table_root.start_address().as_u64(),
    }
}

fn print_backtrace(mut frame_pointer: u64) -> usize {
    println!("GAXERA: PANIC_BACKTRACE_BEGIN");
    let Some(bounds) = active_stack_bounds(frame_pointer) else {
        println!("GAXERA: PANIC_BACKTRACE_END frames=0 reason=outside-known-stack");
        return 0;
    };

    let mut frames = 0;
    while frames < MAX_BACKTRACE_FRAMES {
        if !bounds.contains_frame_pointer(frame_pointer) {
            println!("GAXERA: PANIC_BACKTRACE_END frames={frames} reason=invalid-frame");
            return frames;
        }

        // SAFETY: `frame_pointer` was checked for alignment and for both
        // eight-byte fields to reside inside a static stack mapping. The
        // monotonic-link rule below prevents cycles and descending traversal.
        let (next_frame_pointer, return_address) = unsafe {
            let frame = frame_pointer as *const u64;
            (core::ptr::read(frame), core::ptr::read(frame.add(1)))
        };
        println!("GAXERA: PANIC_BACKTRACE_FRAME index={frames} ip={return_address:#018x}");
        frames += 1;

        if next_frame_pointer == 0 {
            println!("GAXERA: PANIC_BACKTRACE_END frames={frames} reason=chain-end");
            return frames;
        }
        if next_frame_pointer <= frame_pointer {
            println!("GAXERA: PANIC_BACKTRACE_END frames={frames} reason=non-monotonic-chain");
            return frames;
        }
        frame_pointer = next_frame_pointer;
    }

    println!("GAXERA: PANIC_BACKTRACE_END frames={frames} reason=frame-limit");
    frames
}

fn active_stack_bounds(frame_pointer: u64) -> Option<StackBounds> {
    let (bootstrap_start, bootstrap_end) = entry::bootstrap_stack_bounds();
    let bootstrap = StackBounds {
        start: bootstrap_start,
        end: bootstrap_end,
    };
    if bootstrap.contains_frame_pointer(frame_pointer) {
        return Some(bootstrap);
    }

    let (ist_start, ist_end) = descriptors::double_fault_stack_bounds();
    let ist = StackBounds {
        start: ist_start,
        end: ist_end,
    };
    ist.contains_frame_pointer(frame_pointer).then_some(ist)
}

#[cfg(test)]
mod tests {
    use super::StackBounds;

    #[test]
    fn stack_bounds_require_an_aligned_complete_frame() {
        let bounds = StackBounds {
            start: 0x1000,
            end: 0x1100,
        };
        assert!(bounds.contains_frame_pointer(0x1080));
        assert!(!bounds.contains_frame_pointer(0x1081));
        assert!(!bounds.contains_frame_pointer(0x10f8));
        assert!(!bounds.contains_frame_pointer(0x0ff8));
    }
}
