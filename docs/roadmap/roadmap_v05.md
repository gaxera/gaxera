# Gaxera v0.5 Engineering Program

> **Status:** Proposed engineering program. No milestone authorizes code until
> its listed research gates and ADRs are accepted.
> **Baseline:** Gaxera Foundation v0.1, tag `v0.1.0`
> **Purpose:** Define the path from a verified hardware foundation to a small,
> single-processor capability microkernel that can run supervised user programs,
> exchange messages, read an in-memory filesystem, and provide a developer
> console.

## 1. Executive Direction

v0.5 is not a smaller version of a desktop operating system. It is the first
version in which Gaxera's intended microkernel architecture becomes real:

- ring-3 programs run in isolated address spaces;
- every user-visible kernel operation is reached through a capability-bearing
  syscall interface;
- threads can block, wake, and preempt on one bootstrap processor;
- synchronous IPC and notifications connect small user-space services;
- a trusted init program starts a ramfs and developer shell from a boot payload;
- the shell can inspect and read files through IPC, not kernel filesystem code.

The release must remain deliberately constrained. It does not need SMP,
multiple scheduling classes, physical disks, DMA-capable drivers, networking,
graphics, compatibility, persistent users, or production secure boot. It must
make those later systems easier rather than quietly fixing their design in
ad-hoc v0.5 code.

### 1.1 Critical correction to the old horizon

The former horizon ordered context switching and EEVDF scheduling before IPC,
capabilities, and user space. That is the wrong dependency direction. A
scheduler without a task lifecycle, address-space ownership, capability
authority, blocking semantics, or time contract will force later rewrites.

The recommended order is:

```text
object and capability model
  -> user ABI and address-space isolation
    -> task/trap/context ownership
      -> cooperative lifecycle and synchronous IPC
        -> timer calibration and preemption
          -> init payload and user-space services
            -> ramfs and developer shell
```

v0.5 should use a simple single-processor preemptive scheduler with a clear
run-queue abstraction. It should not implement EEVDF, priority inheritance,
deadline scheduling, affinity, or the five long-term classes. Those choices
need workload data, stable IPC blocking semantics, robust timekeeping, and SMP
ownership that v0.5 will not yet have.

## 2. v0.5 Product Boundary

### 2.1 Required outcome

At completion, UEFI QEMU deterministically boots a kernel that loads one
trusted init ELF from a packaged boot payload. Init receives explicit initial
capabilities, starts a ramfs server and shell, and uses capability-gated IPC to
read files. The system demonstrates preemption, user/kernel transitions,
capability denial, IPC call/reply, notification wakeup, service restart after a
controlled crash, and normal shell interaction through the documented
developer-console boundary.

### 2.2 Explicitly out of scope

- SMP, AP startup, x2APIC, IOAPIC routing, MSI/MSI-X, and generic interrupt
  delivery to arbitrary device drivers;
- production timer precision, wall clock, time zones, NTP, vDSO, and all
  long-term scheduling classes;
- DMA, IOMMU, PCI enumeration, USB, GPU, audio, networking, or storage
  drivers;
- persistent filesystem, disk partitioning, CoW, encryption, package manager,
  update system, or account management;
- ELF compatibility, Linux ABI, dynamic linking, POSIX completeness, or
  arbitrary third-party executable loading;
- kernel/user ASLR claims, KASLR relocation, secure boot, signing, and
  physical-hardware security certification;
- capability leases, distributed revocation, resource budgets, and global OOM
  policy beyond interfaces that leave room for them;
- graphics/compositor, windowing, AI/knowledge services, and all user-facing
  product experience beyond the developer shell.

## 3. Target Architecture

```text
                 boot payload (init ELF + read-only ramfs image)
                                      |
UEFI -> Limine -> immutable BootContext -> kernel bootstrap
                                      |
      +-------------------------------+-------------------------------+
      | kernel mechanism                                              |
      |                                                               |
      | Object arena / capability spaces / rights                    |
      | Address spaces / mappings / user-copy boundary                |
      | Threads / trap frames / single-CPU run queue                  |
      | Synchronous endpoints / notifications / timer events          |
      | Bootstrap console bridge (narrow and temporary)               |
      +-------------------------------+-------------------------------+
                                      |
                            syscall + capability ABI
                                      |
      +-------------------------------+-------------------------------+
      | user space                                                     |
      | init supervisor -> ramfs service -> developer shell            |
      |                  -> crash/restart proof service                |
      +---------------------------------------------------------------+
```

The kernel recognizes only mechanism and capability authority. It does not
interpret filesystem paths, command syntax, shell language, or service
protocol payloads. The initial boot payload is a bootstrap transport, not a
kernel filesystem.

### 3.1 Repository evolution

The current single binary crate is appropriate for v0.1 but will become hard
to test as user ABI and object logic grow. At the first v0.5 implementation
milestone, split only along real ownership boundaries:

```text
kernel/                 # bootable kernel binary and architecture glue
crates/kernel-core/     # no_std, host-testable object/cap/IPC/scheduler models
crates/gaxera-abi/      # versioned no_std syscall, handle, message ABI types
user/libgaxera/         # user-side syscall and IPC veneer
user/init/              # trusted initial supervisor
user/ramfs/             # in-memory filesystem service
user/shell/             # minimal developer shell
tools/                  # optional host image/ABI inspection helpers
xtask/                  # image composition, QEMU profiles, evidence capture
docs/architecture/      # stable contracts
docs/abi/               # syscall and boot-payload specifications
docs/roadmap/           # accepted release programs
```

Do not create every directory immediately. Each appears when its owner and
first consumer are accepted. The workspace must continue to use committed
`Cargo.lock`, exact version decisions for architectural dependencies, and
`--locked` in every CI command.

## 4. Engineering Principles

### 4.1 Ownership and isolation

- A capability is authority, not an identifier. Every handle lookup validates
  slot generation, object type, rights, and object liveness.
- Address space, capability space, thread, and scheduling context have
  independently documented lifetimes. A convenience `Process` aggregate must
  not erase those ownership distinctions.
- Kernel objects have one lifecycle owner; cross-object links use validated
  IDs/weak references or explicit reference counts, never unbounded raw
  pointers.
- User pointers are untrusted byte ranges. Copying, mapping, and syscall
  dispatch validate canonicality, range, permissions, overflow, and current
  address-space ownership before dereference.
- User mappings never include HHDM, page tables, kernel heap, kernel stacks,
  boot metadata, or device pages by accident.

### 4.2 Unsafe policy

Unsafe code remains concentrated in architecture modules: entry, CR3/
page-table activation, trap/context assembly, MSRs, port/MMIO access, and
validated user-memory access. Each unsafe operation needs a local invariant,
host-model tests where possible, and a deterministic guest proof where hardware
behavior matters. `kernel-core` should be safe Rust except for carefully
reviewed intrusive storage primitives.

### 4.3 Verification and documentation

Every milestone has a host-model proof and a QEMU proof before it grows a
dependent subsystem. Every new ABI number, object state transition, mapping
permission, and serial success marker belongs in a versioned specification.
Evidence remains append-only and names the exact commit it proves. ADRs record
decisions, not implementation diaries.

### 4.4 Scope discipline

No temporary kernel shortcut becomes permanent by omission. A temporary
bootstrap console capability, if accepted, is labelled transitional, tested,
and listed for removal before general device support. No scheduler policy is
advertised as a real-time contract until timekeeping and IPC inheritance are
designed.

## 5. Research Gates and ADR Schedule

| Gate | Decision to resolve | Required ADR | Proceed only when |
| --- | --- | --- | --- |
| V5-A | Kernel object storage, identity, references, capability-slot and revocation model | ADR 0007 | Object destruction, stale handles, derivation, transfer, and revocation have executable state-machine tests. |
| V5-B | User ABI, syscall entry/exit, trap-frame layout, per-CPU kernel-stack ownership | ADR 0008 | The ABI has a version, register convention, error encoding, canonical-address policy, and hostile-return audit. |
| V5-C | User address-space layout, mapping API, user-copy, initial ELF loading, and ASLR staging | ADR 0009 | Kernel/user mappings, guard pages, copy faults, executable permissions, and init-image trust boundary are specified. |
| V5-D | Thread state machine, context-switch ownership, timer calibration, and single-CPU scheduler contract | ADR 0010 | Blocking/wakeup/preemption races are modeled; no scheduler policy is selected without a usable monotonic tick source. |
| V5-E | Endpoint ABI, notification semantics, capability transfer, cancellation, and deadlock policy | ADR 0011 | Call/reply state diagrams and capability-transfer rollback are independently tested. |
| V5-F | Boot-payload/initramfs format, init authority, service discovery, crash/restart policy | ADR 0012 | The kernel knows only a bounded payload format and init's initial capability set is minimal and reviewable. |
| V5-G | Developer console and input authority: temporary bridge versus port-I/O/IRQ capability model | ADR 0013 | No user service receives ambient I/O privilege; the transitional path has a removal trigger. |
| V5-H | Ramfs service protocol and minimal shell boundary | ADR 0014 | Paths, file data, shell syntax, and service semantics remain user-space and are not smuggled into the kernel. |

Additional ADRs are expected if a gate shows that SMP, IOAPIC, general MMIO,
KASLR, or allocator concurrency is a real dependency. They are not assumed
dependencies today.

## 6. Major Subsystem Decisions

### 6.1 Object and capability core

**Recommended direction:** Gaxera-owned typed object arena plus a per-domain
capability space of generational slots. A capability value encodes a slot and
generation, never a kernel address. The slot stores object reference, type,
rights, and derivation metadata. A derivation tree permits bounded descendant
revocation. Initial v0.5 rights are minimal and object-specific; capability
leases and budgets reserve fields but are not implemented.

**Owner modules:** `kernel-core::object`, `kernel-core::capability`, and
`gaxera-abi::handle`.

**Public API shape:** safe kernel-internal `create`, `derive`, `lookup`,
`transfer_prepare`, `transfer_commit`, `revoke`, and `destroy`; user ABI sees
opaque `Handle` values and operation-specific syscall arguments.

**Internal responsibilities:** maintain typed object lifecycle state, slot
generation and rights validation, derivation links, and all-or-nothing
capability-space mutation. Object storage never decides service policy.

**Invariants:** handles are unforgeable by generation check; rights can only
narrow during derivation; failed transfer leaves both spaces unchanged;
destroyed objects invalidate every lookup; revocation cannot leave a usable
descendant capability.

**Unsafe boundary:** object storage and reference/lifetime implementation only.
No raw pointer crosses a capability lookup boundary.

**Alternatives considered:**

- SeL4-style explicit CSpace traversal is excellent for formal reasoning but
  exposes tree topology and creates a larger early ABI.
- Zircon-like flat handles are ergonomic but need extra derivation bookkeeping
  to make revocation first-class.
- Raw object pointers or integer IDs are rejected because they conflate naming
  with authority and make stale-reference bugs too easy.

The generational-slot model is recommended because it provides a small ABI now
while retaining explicit derivation/revocation structure for later proof work.

**Future extension points:** leases, quotas, object labels, multi-level
capability spaces, and stronger revocation accounting are intentionally fields
or internal implementation changes, not v0.5 ABI commitments.

### 6.2 User ABI, traps, and task execution

**Recommended direction:** x86-64 `syscall`/`sysret` with a shared, explicit
trap-frame representation and a per-CPU kernel stack. Use `swapgs` only with a
documented GS-base policy. All user return addresses and flags must be checked
before `sysret`; invalid cases use an audited `iretq` path.

**Owner modules:** `kernel/src/arch/x86_64/trap.rs`,
`kernel-core::task`, `gaxera-abi::syscall`, and a new `user/libgaxera` veneer.

**Public API shape:** numbered, versioned syscalls for handle operations,
thread control, mapping, endpoint call/reply, notification wait/signal, and
debug-console use if Gate V5-G accepts it. Syscalls return typed ABI status
codes, never kernel pointers.

**Internal responsibilities:** own entry assembly, trap-frame normalization,
dispatch, return-path selection, and all user-register validation. The ABI
crate owns wire layouts; kernel services own syscall semantics.

**Invariants:** kernel stack is selected before Rust handler code; user RSP/RIP
are canonical and user-range; direction flag and privileged flags are
sanitized; user ABI buffers are validated; no `sysret` target can fault into an
unsafe privilege transition.

**Alternatives considered:**

- `int` gates reuse existing IDT code but add interrupt-gate semantics and are
  an inferior long-term syscall fast path.
- Call gates are obsolete and poorly aligned with modern x86-64 practice.
- `syscall`/`sysret` is fastest and conventional but has sharp GS and return
  validation requirements; it is recommended only after Gate V5-B proves them.

**Future extension points:** ABI version negotiation, batched operations,
vDSO-like read-only data, and architecture-specific fast paths require an ABI
revision or an explicitly reserved extension mechanism.

### 6.3 Address spaces and memory objects

**Recommended direction:** retain the v0.1 kernel higher-half mapping as a
supervisor-only shared region, create a distinct CR3 root per user address
space, and expose mappings only through `MemoryObject` plus capability rights.
Use 4 KiB pages first. Initial user mappings are fixed and deterministic; user
ASLR is a Gate V5-C decision. KASLR is explicitly deferred rather than
silently implied.

**Owner modules:** `kernel-core::address_space`, `kernel-core::memory_object`,
and architecture-specific page-table application in `paging.rs`.

**Public API shape:** create address space, create/frame-backed memory object,
map/unmap a page-aligned range with read/write/execute rights, and copy to/from
user through checked slices. No public arbitrary-frame mapper exists.

**Internal responsibilities:** own mapping metadata, page-table application,
TLB invalidation, memory-object lifetime, and checked user-copy mechanics.
The physical allocator remains the sole source of page-table frames.

**Invariants:** one physical frame is not mapped writable into unrelated
domains without an explicit shared-memory capability; W^X applies to user
mappings; user addresses cannot overlap kernel slots; unmap invalidates TLB
state; page-table frames remain kernel-owned.

**Alternatives considered:**

- A single shared address space is easier but makes capability isolation
  cosmetic and is rejected.
- Reusing the v0.1 kernel CR3 for user tasks is rejected because it leaks the
  bootstrap memory model into the security boundary.
- Huge pages and demand paging are deferred because they add policy and failure
  modes before basic mapping ownership is proven.

**Future extension points:** copy-on-write, shared-memory grants, lazy paging,
ASLR, huge pages, and page-table reclamation build on `MemoryObject` rather
than widening the initial mapping API.

### 6.4 Threads, scheduler, and time

**Recommended direction:** a single-BSP scheduler with explicit thread states
(`New`, `Runnable`, `Running`, `Blocked`, `Dying`, `Dead`), intrusive run and
wait queues owned by the scheduler, cooperative switching first, then APIC
timer preemption after Gate V5-D accepts calibration. v0.5 uses FIFO within a
small number of fixed priority bands or a simple fair queue selected by the
gate; it does not promise EEVDF.

**Owner modules:** `kernel-core::thread`, `kernel-core::scheduler`,
`kernel-core::time`, and `arch/x86_64/context.rs` / `apic.rs`.

**Public API shape:** create/start/yield/block/exit threads; wait/signal
notifications; scheduler-visible timer arm/cancel only through `TimerObject`.

**Internal responsibilities:** own task state transitions, run/wait queues,
context lifetime, timer-to-reschedule handoff, and reaping. Scheduler policy
does not own IPC message contents or capability decisions.

**Invariants:** exactly one thread is `Running` on the BSP; a blocked thread is
not on a run queue; a wakeup is not lost across state changes; switch assembly
saves/restores every ABI-required register; no lock is held across a context
switch; timer IRQ does bounded accounting then selects a safe reschedule point.

**Alternatives considered:**

- EEVDF immediately is rejected: its virtual-time accounting and fairness
  claims require stable clock and IPC semantics.
- Fully cooperative scheduling is useful for the first context-switch proof
  but cannot be the v0.5 end state because one buggy service can starve others.
- SMP-first scheduling is rejected because it would force allocator, per-CPU,
  TLB, interrupt, and lock design before a single-core lifecycle is trusted.

**Future extension points:** SMP run queues, priority inheritance,
deadline-aware scheduling, EEVDF, and accounting become separate design work
after a monotonic clock and IPC blocking behavior are proven.

### 6.5 IPC and notifications

**Recommended direction:** synchronous bounded call/reply endpoints plus
one-way notifications with a pending-bit mask. Messages are fixed-size inline
bytes for v0.5; large data uses mapped `MemoryObject` pages. Capability transfer
uses an explicit prepare/commit/rollback transaction. No kernel broadcast,
semantic decoding, or arbitrary cancellation is introduced initially.

**Owner modules:** `kernel-core::ipc`, `kernel-core::notification`,
`kernel-core::capability`, and `gaxera-abi::ipc`.

**Public API shape:** endpoint create, send/receive/call/reply, notification
signal/wait, memory-object map, and optional capability transfer array with a
small fixed maximum established by benchmarking.

**Internal responsibilities:** own endpoint wait state, reply-token lifetime,
notification-bit coalescing, and transactional capability transfer. Service
protocol parsing and payload interpretation stay outside the kernel.

**Invariants:** every blocked caller has exactly one endpoint wait state;
reply authority cannot be forged or reused; transfer failure is atomic; kernel
does not interpret payload bytes; endpoint destruction wakes waiters with a
defined error; notifications coalesce bits without allocating in IRQ context.

**Alternatives considered:**

- Async queues first require buffering, quotas, cancellation, and overflow
  policy; they are deferred.
- Shared memory only cannot express control flow or authority transfer.
- Copying arbitrary user pointers is rejected; inline messages and explicit
  mapping make ownership visible.

**Future extension points:** asynchronous queues, cancellation, priority
inheritance, scatter/gather payloads, and shared-memory rings require distinct
resource and wakeup policies before they can extend this base.

### 6.6 Boot payload, init, and services

**Recommended direction:** package a small, versioned boot payload as Limine
modules: one trusted init ELF and one read-only ramfs image. Extend only the
architecture entry layer to copy module descriptors into `BootContext`. The
kernel minimally loads the fixed init ELF into a fresh address space and gives
it a narrow initial capability set. Init starts the ramfs and shell, owns names
and restart policy, and remains the only service discovery authority in v0.5.

**Owner modules:** `boot.rs`, `kernel-core::boot_payload`,
`kernel-core::elf_loader`, `user/init`, and `xtask` image composition.

**Public API shape:** a private boot payload parser and a documented initial
capability manifest; user-space service discovery occurs through init endpoint
capabilities, not global kernel names.

**Internal responsibilities:** copy and validate boot descriptors, parse only
the bounded manifest and ELF subset, construct init's initial objects, and
then relinquish service naming and lifecycle policy to init.

**Invariants:** only boot.rs reads Limine module responses; each payload range
is checked against immutable boot metadata; init receives no ambient physical
memory, page-table, raw APIC, or unrestricted debug capability; a service
crash cannot overwrite kernel or peer address spaces.

**Alternatives considered:**

- Embedding init in the kernel complicates replacement and blurs user/kernel
  boundary.
- A full package manager or dynamic linker is out of scope.
- Loading arbitrary ELF files before a trust and resource model exists is
  rejected. The fixed boot payload is recommended as a bounded bootstrap.

**Future extension points:** signed payload manifests, multiple modules,
dynamic loading, and richer restart supervision are deferred until their trust
and resource-accounting requirements are separately accepted.

### 6.7 Developer console, ramfs, and shell

**Recommended direction:** ramfs and shell are user-space processes. The
kernel exports neither path parsing nor file semantics. Gate V5-G decides the
temporary developer-console path. The preferred transitional option is a
capability-gated bootstrap console service backed by the existing QEMU COM1
mechanism, explicitly unavailable to ordinary tasks and scheduled for removal
when the port-I/O/IRQ authority model is accepted.

**Owner modules:** `user/ramfs`, `user/shell`, `user/init`; narrowly scoped
bootstrap console glue only if Gate V5-G accepts it.

**Public API shape:** ramfs request/reply protocol for open/read/list; shell
uses only service endpoint capabilities and no filesystem kernel syscall.

**Internal responsibilities:** init owns service routing, ramfs owns image
format and path semantics, and the shell owns command syntax and interaction.
Any bootstrap console glue owns only its narrowly granted output/input bridge.

**Invariants:** shell input/output does not grant ambient I/O privilege; ramfs
paths and command parsing remain user-space; a malformed file request cannot
corrupt service memory; a console capability does not imply general port I/O.

**Alternatives considered:**

- Kernel-resident shell/ramfs is rejected as a direct violation of the future
  microkernel split.
- Immediate user-space PS/2 support requires a port-I/O and IRQ design that
  deserves its own ADR; it must not be smuggled in as shell plumbing.
- A scripted-only shell is insufficient for the v0.5 developer milestone.

**Future extension points:** a real device-capability model, interrupt-backed
input, writable filesystems, process launch policy, and richer shells remain
user-space work and must not enlarge the kernel's console bridge.

## 7. Milestone Program

### M0: Program setup and baseline preservation

**Objective:** establish v0.5 documentation, ABI versioning rules, workspace
split plan, and test harness conventions without changing v0.1 behavior.

**Dependencies:** v0.1 tags and exact evidence.

**Scope:** Foundation reference, this program, ADR index policy, host test
conventions, QEMU profile naming, and an explicit v0.1 regression job.

**Verification/evidence:** `v0.1.0` matrix remains runnable; document links
are checked; immutable evidence is not rewritten.

**Exit:** Gates V5-A through V5-H have named owners, ADR templates, and no
code milestone begins without its gate.

### M1: Object lifecycle and capability-space model

**Objective:** implement the accepted V5-A object arena and capability state
machine in host-testable `kernel-core`.

**Dependencies:** ADR 0007.

**Scope:** object IDs, capability slots, type/rights checks, derivation,
destroy, stale-handle detection, transfer transactions, and revocation model.

**Unsafe invariants:** arena storage cannot move live object identity; slot
reuse requires generation change; reference accounting cannot underflow.

**Verification/evidence:** exhaustive state-table tests plus property tests for
derive/revoke/transfer sequences; no QEMU dependency yet.

**Exit:** every object lookup path has deterministic success and denial tests;
no raw object pointer crosses an API boundary.

### M2: User ABI and isolated address spaces

**Objective:** establish V5-B/V5-C and enter a minimal ring-3 probe in its own
address space through an audited syscall path.

**Dependencies:** M1, ADRs 0008 and 0009.

**Scope:** ABI crate, trap entry/exit, per-CPU kernel stack, user page tables,
checked user copy, `MemoryObject` mapping, and a fixed trusted user ELF probe.

**Unsafe invariants:** all user return and pointer checks precede privileged
use; kernel mappings stay supervisor-only; TLB state matches mapping changes.

**Verification/evidence:** host range/permission tests; QEMU profiles for user
entry, syscall round trip, denied invalid handle, bad user pointer, bad return
address, user guard-page fault, and clean return to kernel.

**Exit:** an unprivileged probe cannot read/write kernel or HHDM addresses and
can only invoke a capability-authorized no-op/yield syscall.

### M3: Threads and cooperative execution

**Objective:** prove context ownership and task lifecycle before timer-driven
preemption.

**Dependencies:** M2 and ADR 0010's cooperative portion.

**Scope:** thread objects, kernel stacks, context save/restore, start/yield,
block/wake/exit transitions, one BSP run queue, and deterministic two-task
user proof.

**Unsafe invariants:** saved context contains all required state; a dead task
is never scheduled; no thread frees its current stack; queue links have one
owner.

**Verification/evidence:** host state-machine/property tests; QEMU alternation
markers from two isolated tasks and deliberate task exit/reap proof.

**Exit:** two user tasks can yield in a deterministic order without corrupting
kernel or each other's state.

### M4: Endpoint IPC, notifications, and capability transfer

**Objective:** implement V5-E mechanism without scheduler policy leakage.

**Dependencies:** M1 through M3 and ADR 0011.

**Scope:** endpoint and notification objects, synchronous call/reply, blocking
wakeup, reply authority, fixed inline message ABI, capability transfer, and
endpoint teardown behavior.

**Unsafe invariants:** caller/replier state transitions are atomic under the
single-CPU execution model; transfer rollback preserves both capability spaces;
notifications remain nonallocating.

**Verification/evidence:** host transition matrix/property tests; QEMU request,
reply, denied-rights, transferred-capability, destroyed-endpoint, and
notification coalescing profiles.

**Exit:** the kernel transports bytes and capability references only; no test
depends on kernel interpretation of a service protocol.

### M5: Time source, timer object, and preemptive single-CPU scheduler

**Objective:** turn the Phase 5 delivery proof into an accepted, bounded
preemption mechanism without claiming production timekeeping.

**Dependencies:** M3, M4, completed V5-D calibration decision.

**Scope:** selected monotonic tick source, APIC timer programming, minimal
timer object, reschedule request from IRQ, safe switch point, and simple fair
or priority-band policy accepted by ADR 0010.

**Unsafe invariants:** interrupt handler does no allocation or blocking; timer
state cannot wake a dead thread; a preemption request cannot switch from an
inconsistent kernel stack; EOI behavior remains correct.

**Verification/evidence:** deterministic QEMU CPU-bound versus yielding task
proof, timer expiry wakeup, no-lost-wakeup stress loop, and scheduler fairness
trace. Record calibration source and limits in evidence.

**Exit:** a non-yielding user task cannot starve a second runnable task on the
BSP. No EEVDF or real-time latency claim is made.

### M6: Boot payload and init supervisor

**Objective:** replace test-only user probes with a bounded init process and
explicit initial authority.

**Dependencies:** M1 through M5, ADR 0012.

**Scope:** copied Limine module descriptors in BootContext, payload manifest,
minimal ELF loader, initial address space/thread/capability space, init service
registry, and controlled child-service crash/restart proof.

**Unsafe invariants:** module boundaries are checked; ELF segments obey W^X;
init cannot obtain unrestricted kernel/system authority by handle forgery;
restart reclaims or invalidates all dead-service capabilities and mappings.

**Verification/evidence:** malformed module/ELF host tests; QEMU init-ready,
least-authority manifest, service crash, revocation, and restart markers.

**Exit:** the kernel boots init from packaged data and knows no service name or
shell/ramfs protocol after that handoff.

### M7: Ramfs service and developer shell

**Objective:** reach the v0.5 usable developer surface without moving file or
shell policy into the kernel.

**Dependencies:** M4 through M6 and ADRs 0013/0014.

**Scope:** read-only in-memory filesystem image, ramfs list/open/read protocol,
capability-routed service discovery, shell command loop, `help`, `ls`, `cat`,
and `echo`, plus the accepted limited developer-console path.

**Unsafe invariants:** console bridge does not grant ambient port access;
ramfs parser bounds checks every image offset; shell buffers remain user-space;
service IPC payload validation is service-owned.

**Verification/evidence:** ramfs parser host fuzz/property corpus; QEMU shell
script that lists and reads seeded files, capability denial to an unrelated
task, console round trip, and ramfs-service restart/reconnect proof.

**Exit:** a user can interact with the shell in QEMU and read a bundled file
through ramfs IPC. The kernel contains neither filesystem paths nor commands.

### M8: v0.5 audit and release

**Objective:** independently audit architecture, unsafe boundaries, ABI,
evidence, docs, reproducibility, and deferred work before release.

**Dependencies:** M0 through M7.

**Scope:** review every new unsafe block, API, rights check, transition,
feature profile, workspace dependency, and document. No feature work.

**Verification/evidence:** clean exact-commit matrix, host property/fuzz tests,
QEMU integration matrix, optional physical-machine smoke test clearly separated
from required evidence, release provenance, and tag-triggered CI.

**Exit:** all v0.5 exit criteria below are backed by exact-commit evidence and
the repository contains no undocumented temporary authority path.

## 8. Verification Strategy

### 8.1 Host verification

Move pure lifecycle, capability, IPC, address-range, ELF-header, ramfs-parser,
and scheduler-state logic into `kernel-core` so it can be unit tested without
privileged hardware. Add deterministic property tests for sequences, not just
examples: allocate/destroy/reuse, derive/revoke, send/reply/cancel, wake/exit,
and map/unmap/fault.

Use parser fuzzing for boot payload, ELF, and ramfs formats once their byte
formats exist. Fuzz targets must never exercise privileged code or claim to
validate hardware behavior.

### 8.2 Guest verification

Extend `xtask` from individual boot profiles into named scenarios with
machine-readable markers and explicit expected exit statuses. Scenarios should
use a deterministic initramfs and bounded instruction/timeout budget. A guest
scenario must prove both positive behavior and an expected denial or fault for
every new authority boundary.

Required v0.5 scenario families are:

- user entry and hostile-syscall return validation;
- address-space isolation and user guard pages;
- context switch and preemption;
- capability denial, transfer, revocation, and stale handle rejection;
- endpoint call/reply, notification, destroyed peer, and no-lost-wakeup;
- init bootstrap, service restart, ramfs read, and shell interaction;
- full v0.1 regression matrix unchanged.

### 8.3 CI and reproducibility

CI should retain a fast formatting/lint/host-test job and add a slower UEFI
scenario job. It must build every user ELF and the payload image from the
committed lockfile. Release CI should perform a clean bootstrap and validate
that normal ISO restoration did not leave a test image staged.

Pin GitHub Actions by commit digest before v0.5 release hardening, preserve the
committed Limine checksum, record QEMU/OVMF versions in evidence, and add ABI
compatibility checks that reject accidental wire-layout changes without an ABI
version update.

## 9. Technical Risk Register

| Risk | Why it matters | Mitigation and gate |
| --- | --- | --- |
| Capability revocation semantics are wrong | A later security model cannot repair leaked authority cheaply. | V5-A state models, generation tests, explicit derivation tree, ADR 0007. |
| `sysret`/`swapgs` error permits privilege confusion | This is a high-severity x86 boundary. | V5-B hostile-return audit, tiny assembly surface, QEMU negative tests, ADR 0008. |
| User-copy faults reenter an unsafe kernel state | Copying user bytes is a new faulting path. | V5-C recovery design, fault labels or checked mappings, QEMU bad-pointer tests. |
| Context switch corrupts registers/stacks | Failure can be intermittent and nonlocal. | Cooperative proof first, fixed register contract, disassembly review, deterministic alternation test. |
| Preemption races lose wakeups | Scheduler correctness is defined by rare transitions. | Single CPU first, state-machine tests, explicit IRQ-to-scheduler handoff, stress profiles. |
| IPC transfer leaves split authority | Capability duplication or loss breaks isolation. | Two-phase transfer with rollback and property tests, V5-E. |
| Bootstrap console becomes a permanent kernel driver | It would violate the intended user-space device model. | Gate V5-G, least-authority cap, removal criterion, no generic port API by accident. |
| Boot payload turns into an undocumented package system | Trust and parsing complexity can grow silently. | Versioned bounded manifest, fixed init only, V5-F. |
| v0.1 static mapping assumptions leak into user VM | HHDM or bootstrap data exposure defeats isolation. | New address-space types, no direct mapper exposure, mapping denial tests. |
| Scope expands into SMP, disk, or graphics | It would delay core microkernel proof indefinitely. | Explicit out-of-scope list; ADR required for every claimed dependency. |

## 10. Technical Debt Strategy

### Address during v0.5

- Replace the v0.1 single bootstrap execution model with explicit task and
  kernel-stack ownership.
- Turn the delivery-only APIC timer into a bounded single-CPU preemption source
  only after calibration is accepted.
- Extend BootContext to copy only the module metadata required for a boot
  payload; continue to prohibit leaked Limine pointers.
- Introduce a safe, typed address-space/memory-object layer instead of exposing
  v0.1 paging helpers to user-facing code.
- Separate host-testable logic from bootable architecture glue.

### Preserve as intentional deferral

- x2APIC/SMP/IOAPIC/MSI and general MMIO;
- HHDM expansion, page-table-node reclamation, huge pages, and concurrent
  physical allocation;
- physical UART reliability and broad framebuffer support;
- persistent storage, resource accounting, leases, secure boot, KASLR, IOMMU,
  and general device authority;
- multi-class scheduling, priority inheritance, and performance claims.

## 11. v0.5 Exit Criteria

Gaxera v0.5 is complete only when all of the following are true:

1. Every V5 research gate has an accepted ADR or a documented decision to
   defer its dependent milestone.
2. The kernel starts at least three isolated ring-3 programs under a trusted
   init process on UEFI QEMU.
3. Every user-visible kernel operation requires a checked capability; tests
   prove denial, stale-handle rejection, rights narrowing, transfer, and
   revocation behavior.
4. Each user program has a distinct address space; tests prove that user
   pointers, guard pages, and mapping permissions cannot reach kernel/HHDM
   memory or another task's private pages.
5. A single-BSP scheduler performs deterministic cooperative switching and
   timer-driven preemption so a non-yielding task cannot starve another.
6. Endpoints provide synchronous call/reply and notifications provide bounded
   asynchronous wakeups; tests cover lifecycle error paths and capability
   transfer rollback.
7. The kernel loads a bounded boot payload into init. Init starts, detects, and
   restarts a controlled crashing service without rebooting the kernel.
8. A user-space ramfs service and developer shell can list and read seeded
   in-memory files through IPC. No kernel code parses paths or shell commands.
9. Host model/property/parser tests and the deterministic UEFI QEMU scenario
   matrix pass from a clean locked build. v0.1 profiles remain passing.
10. Exact-commit evidence, ADRs, ABI specs, workflow, environment, handoffs,
    governance records, and release CI are synchronized. Every temporary
    authority path is either removed or explicitly listed with a successor.

Meeting these criteria establishes an operating microkernel development system,
not a secure desktop or production OS. That distinction is part of the release
contract.
