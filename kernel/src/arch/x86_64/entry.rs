#[cfg(target_os = "none")]
use core::arch::global_asm;

// Debug-mode early boot includes large fixed-size allocator construction
// frames. Keep 64 KiB plus an unmapped guard page until later stack growth is
// introduced with per-thread kernel stacks.
pub const BOOTSTRAP_STACK_SIZE: usize = 64 * 1024;

#[repr(align(16))]
#[allow(dead_code)] // Referenced by the `_start` assembly trampoline.
struct Stack([u8; BOOTSTRAP_STACK_SIZE]);

// This allocation is deliberately in its own linker section. Phase 4's owned
// page tables leave the preceding guard page absent before this stack is used.
#[unsafe(link_section = ".bootstrap_stack")]
#[used]
static BOOTSTRAP_STACK: Stack = Stack([0; BOOTSTRAP_STACK_SIZE]);

#[cfg(target_os = "none")]
global_asm!(
    r#"
    .section .text.boot_entry,"ax"
    .global _start
    .type _start,@function
_start:
    lea rsp, [rip + __bootstrap_stack_end]
    and rsp, -16
    call gaxera_rust_entry
    ud2
"#
);
