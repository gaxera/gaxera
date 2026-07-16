# ADR 0002: x86-64 Descriptor Tables and Exception Policy

## Status

Accepted

## Context

Phase 3 must replace Limine's temporary descriptor state with Gaxera-owned
GDT, TSS, and IDT tables. The kernel needs a dedicated, statically allocated
Interrupt Stack Table (IST) stack for processor-delivered double faults.
Descriptor encoding, table loads, segment reloads, and interrupt ABI details
are architecture-sensitive and unsafe to reproduce ad hoc.

## Decision

Gaxera uses the exact dependency below for x86-64 descriptor-table and
interrupt primitives:

```toml
x86_64 = { version = "=0.15.5", default-features = false, features = ["instructions", "abi_x86_interrupt"] }
```

The crate owns descriptor layouts, selector and address types, table-load
instructions, segment helpers, the Rust x86 interrupt ABI, and typed
interrupt-stack-frame representations. Gaxera owns table and stack lifetime,
initialization order, selector and vector policy, handler behavior, unsafe
invariants, fault injection, and all validation.

Phase 3 installs a Gaxera-owned flat 64-bit kernel code segment, kernel data
segment, and TSS. The TSS IST slot 0 points to one static, aligned, writable
double-fault stack in `.bss`. The double-fault IDT entry alone uses that IST
slot in this phase. Interrupts remain disabled; Phase 3 does not configure the
PIC, APIC, or execute `sti`.

Breakpoint is resumable and returns with `iretq`. Division error, invalid
opcode, general protection fault, page fault, and double fault are terminal
in Phase 3: they emit structured serial diagnostics and halt, or perform a
feature-gated QEMU test exit after the diagnostic. The double-fault criterion
is delivery to this handler on IST slot 0 without a triple fault. The handler
reads RSP before emitting its success marker and rejects a value outside the
static IST allocation. The test image intentionally leaves its page-fault gate
non-present, then causes #PF;
the resulting #PF/#NP exception-delivery pair is escalated by the processor to
#DF. Production images always install the page-fault handler. A software
`int $8` does not satisfy that criterion.

QEMU runs all test images with `-no-reboot`, a 20-second absolute deadline,
and `isa-debug-exit` at port `0xf4`. A triple fault therefore becomes a failed
host process rather than an automatic reboot that could obscure the failure.

UEFI QEMU is the required Phase 3 validation target. BIOS remains optional
packaging diagnostics only. The exception test harness must use bounded QEMU
execution and guest-selected `isa-debug-exit` status codes; host-side process
killing after a marker is not sufficient evidence for exception handling.

## Consequences

The new dependency expands the kernel's trusted build input, so its exact
version and Cargo.lock checksum are reviewed and committed. It removes the
need for handwritten descriptor encodings but does not make descriptor-table
configuration safe by itself. Every unsafe initialization operation requires a
local invariant comment and QEMU validation.

Moving deterministic QEMU exits into Phase 3 intentionally changes the
roadmap ordering. This creates reliable positive and negative outcomes before
the project begins testing terminal hardware exceptions.
