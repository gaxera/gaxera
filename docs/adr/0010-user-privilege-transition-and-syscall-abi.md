# ADR 0010: User Privilege Transition and Syscall ABI

**Status:** Accepted
**Date:** 2026-07-18
**Deciders:** Gaxera project

## Context

The v0.1 foundation owns a bootstrap-processor GDT, TSS, IDT, and a static
kernel stack, but it has never entered ring 3. A later public syscall ABI must
not be the first code to prove selector correctness, privilege-stack switching,
or a return to kernel control. Combining those concerns would make a defect in
`syscall`/`sysret`, GS-base handling, or user-memory copying indistinguishable
from a basic privilege-transition failure.

M2A needs a deliberately narrow proof that an unprivileged instruction stream
runs in a distinct address space, traps onto a Gaxera-owned kernel stack, and
returns to a Gaxera-owned continuation. It must not accidentally become a
public ABI merely because it uses an x86 trap mechanism.

## Decision

M2A uses a fixed internal ring-3 probe entered exclusively with an audited
`iretq` frame. It does not expose a public syscall ABI.

The x86-64 architecture layer owns the bootstrap processor's kernel/user GDT
selectors, static TSS storage, `RSP0` installation, the transition frame, and
the internal test-return gate. A thread will later own its kernel-stack
allocation; M2A owns exactly one static transition stack with a lower unmapped
guard page. The architecture layer sets `TSS.RSP0` to that stack's aligned top
immediately before every ring-3 entry and treats it as immutable until the
probe returns or terminates.

M2A adds ring-3 code and data selectors to the existing GDT. The code selector
is execute/read at DPL 3; the data selector is read/write at DPL 3. Kernel
selectors remain DPL 0. The TSS descriptor remains Gaxera-owned and loaded
before any user entry. The existing double-fault IST contract is unchanged.

The M2A entry frame is constructed only from fixed, validated values:

- user RIP is the fixed probe entry address;
- user RSP is the aligned top of the fixed user stack mapping;
- CS and SS are the M2A ring-3 selectors;
- RFLAGS starts at architectural bit 1 only, with IOPL, DF, NT, AC, and
  privileged/undefined control state cleared; interrupts remain disabled for
  the proof;
- all addresses must be canonical, within the documented user range, and
  page-aligned where required by mapping policy.

The probe begins with `int3`, proving that an IDT entry from ring 3 switches to
the TSS-provided kernel stack and can return with `iretq`. It then invokes one
test-only DPL-3 return vector. That vector is not a syscall number, accepts no
arguments, is reachable only from the fixed M2A probe image, switches back to
the kernel CR3/continuation, and is absent from the future public ABI. A
separate negative probe deliberately executes a privileged instruction and
must reach the general-protection handler as CPL 3 without executing kernel
continuation code.

M2B, not M2A, defines the numbered, versioned register-only syscall ABI. M2B
may use `syscall` entry only after it specifies the STAR/LSTAR/FMASK state,
GS-base policy, `swapgs`, hostile return checks, `sysret` eligibility, and
audited `iretq` fallback. No M2A implementation may use `syscall`, `sysret`,
`swapgs`, user pointers, or fault recovery. User extended-state instructions
remain forbidden until M3 supplies save/restore ownership.

## Consequences

The first privilege proof has a small assembly surface: one entry frame, one
IDT-based internal return gate, and one controlled kernel continuation. It
proves that `TSS.RSP0` is live before public ABI complexity exists. A user
cannot gain I/O, interrupt, page-table, kernel stack, HHDM, or general kernel
authority from the probe.

M2A does not create a stable user-facing calling convention. User programs,
libgaxera, syscalls, capability lookup, user copies, and IPC remain absent.
The test return vector must be removed or remain compile-time test-only before
any general user payload is introduced.

## Alternatives Considered

**Introduce `syscall`/`sysret` immediately:** rejected because return-address
validation, GS-base state, and fault recovery would obscure the basic ring-3
and `RSP0` proof.

**Use a public `int 0x80` ABI first:** rejected because it would prematurely
freeze an interrupt-gate ABI and blur a test escape hatch with a system-call
contract.

**Use `ud2` or a fatal privileged instruction as the only user exit:** rejected
because it proves a trap but not a clean return to normal kernel control.

**Reuse the bootstrap stack as `RSP0`:** rejected because the active boot path,
panic telemetry, and future thread ownership would share an ambiguous stack
lifetime. M2A needs one named transition-stack owner.
