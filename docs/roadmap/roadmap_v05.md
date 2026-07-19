# Gaxera v0.5 Engineering Program

> **Status:** Frozen implementation architecture. Changes require an ADR or
> implementation evidence showing a genuine design flaw.
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
- a deterministic scripted developer session can inspect and read files through
  IPC, not kernel filesystem code.

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
developer-trusted static init image from a packaged boot payload. Init receives explicit initial
capabilities, starts a ramfs server and shell, and uses capability-gated IPC to
read files. The system demonstrates preemption, user/kernel transitions,
capability denial, IPC call/reply, notification wakeup, service restart after a
controlled crash, and a deterministic scripted developer session through the
documented output-only console boundary.

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
- capability leases, distributed revocation, dynamic resource budgets, and
  global OOM policy beyond v0.5's bounded `ResourceDomain` limits;
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
      | Resource domains / object arena / capability spaces / rights |
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
- After bootstrap, every user-triggerable allocation is fallible. Resource
  exhaustion returns a defined error; it must not invoke the fatal allocator
  path or panic the kernel.

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
decisions, not implementation diaries. A formal requirements-trace document
must exist to track what v0.5 implements, represents in API shape, and explicitly defers.

### 4.4 Scope discipline

No temporary kernel shortcut becomes permanent by omission. A temporary
bootstrap console capability, if accepted, is labelled transitional, tested,
and listed for removal before general device support. No scheduler policy is
advertised as a real-time contract until timekeeping and IPC inheritance are
designed.

## 5. Research Gates and ADR Schedule

| Gate | Decision to resolve | Required ADR | Proceed only when |
| --- | --- | --- | --- |
| V5-A | Kernel object storage, identity, delegation, and hybrid revocation model | ADR 0007 | Derivation tree, generational slots, and physically lazy revocation with in-flight operation rules are defined. |
| V5-B | User ABI, syscall entry/exit, trap-frame layout, TSS `RSP0` ownership | ADR 0008 | The ABI has a version, register convention, error encoding, and hostile-return audit. |
| V5-C | User address-space layout, mapping API, initial ELF loading, and ASLR staging | ADR 0009 | Kernel/user mappings, executable permissions, and init-image trust boundary are specified. |
| V5-D | Thread state machine, context-switch ownership, FPU/xstate policy, and timer calibration | ADR 0010 | Trap frames (transient) and contexts (persistent) are distinct; blocking races are modeled. |
| V5-E | Endpoint ABI, notification semantics, capability transfer, and deadlock policy | ADR 0011 | Call/reply state diagrams and capability-transfer rollback are independently tested. |
| V5-F | Boot-payload handoff, init authority manifest, service discovery, crash/restart policy | ADR 0012 | Init authority is strictly bounded; kernel loads only init, and init owns archive interpretation. |
| V5-G | Developer console and input authority: temporary bridge versus port-I/O/IRQ capability model | ADR 0013 | No user service receives ambient I/O privilege; the transitional path has a removal trigger. |
| V5-H | Ramfs service protocol and minimal shell boundary | ADR 0014 | Shell is deterministic and scripted. Paths and file data remain user-space and are not smuggled into the kernel. |
| V5-I | User Image and Artifact Pipeline | ADR 0017 | User target, static ELF format, relocation, payload manifest, and host-side composition are defined. |
| V5-J | User-memory access and page-fault recovery | ADR 0015 | A narrowly scoped, non-nestable fault-recovery state exists for explicitly marked user-copy routines. |
| V5-K | Kernel object allocation and resource domains | ADR 0016 | `ResourceDomain` is introduced as the 11th object; all user-triggerable allocations are fallible. |

Additional ADRs are expected only if implementation shows SMP,
IOAPIC, general MMIO, KASLR, or allocator concurrency is a real dependency.

## 6. Major Subsystem Decisions

### 6.1 Object and capability core

**Recommended direction:** a first-class `ResourceDomain` owns bounded
allocation authority. Gaxera-owned typed object arenas are charged to it, and
each domain has capability spaces of generational slots. A capability value
encodes a slot and generation, never a kernel address. The slot stores object
reference, type, rights, and derivation metadata. A bounded derivation lineage
permits logically immediate descendant revocation with deferred cleanup.

**Owner modules:** `kernel-core::resource`, `kernel-core::object`,
`kernel-core::capability`, and `gaxera-abi::handle`.

**Public API shape:** safe kernel-internal `create`, `derive`, `lookup`,
`transfer_prepare`, `transfer_commit`, `revoke`, and `destroy`; creation takes
a `ResourceDomain` allocation authority and is fallible. A Factory is a right
on a `ResourceDomain` capability, not a separate kernel object. User ABI sees
opaque `Handle` values and operation-specific syscall arguments.

**Internal responsibilities:** maintain typed object lifecycle state, fallible resource allocation, slot
generation and rights validation, derivation links, and all-or-nothing
capability-space mutation. Object storage never decides service policy.

**Invariants:** handles are unforgeable by generation check; rights can only
narrow during derivation; failed transfer leaves both spaces unchanged;
destroyed objects invalidate every lookup; once revocation returns, every
future use of a descendant fails; exhaustion never panics a running kernel.

**Unsafe boundary:** object storage and reference/lifetime implementation only.
No raw pointer crosses a capability lookup boundary.

**Alternatives considered:**

- SeL4-style explicit CSpace traversal is excellent for formal reasoning but
  exposes tree topology and creates a larger early ABI.
- Zircon-like flat handles are ergonomic but need extra derivation bookkeeping
  to make revocation first-class.
- Raw object pointers or integer IDs are rejected because they conflate naming
  with authority and make stale-reference bugs too easy.

The generational-slot plus bounded-lineage model is recommended because it
provides a small ABI, rejects stale handles, gives immediate authorization
semantics, and avoids unbounded revoke-time walks. `ResourceDomain` is the
eleventh kernel object because neither `CapabilitySpace` nor `AddressSpace`
has the correct lifetime to own allocation accounting.

**Future extension points:** leases, hierarchical domains, dynamic quotas,
object labels, multi-level capability spaces, and stronger reclamation are
intentionally deferred. The v0.5 domain interface reserves their ownership
boundary without claiming their policy.

### 6.2 User ABI, traps, and task execution

**Recommended direction:** first prove ring transition with a fixed internal
probe, then expose x86-64 `syscall` entry with a shared explicit trap-frame
contract and a per-CPU kernel stack. Architecture code owns user selectors,
GDT/TSS programming, and loading the current thread's `RSP0`; a thread owns
its kernel-stack storage. Use `swapgs` only with a documented GS-base policy.
All user return addresses and flags must be checked before `sysret`; invalid
cases use an audited `iretq` path.

**Owner modules:** `kernel/src/arch/x86_64/trap.rs`,
`kernel-core::task`, `gaxera-abi::syscall`, and a new `user/libgaxera` veneer.

**Public API shape:** numbered, versioned syscalls for handle operations,
thread control, mapping, endpoint call/reply, notification wait/signal, and
debug-console use if Gate V5-G accepts it. Syscalls return typed ABI status
codes, never kernel pointers. Initial IPC uses register-only payloads until ADR 0015.

**Internal responsibilities:** own entry assembly, trap-frame normalization,
dispatch, return-path selection, and all user-register validation. The ABI
crate owns wire layouts; kernel services own syscall semantics.

**Invariants:** kernel stack is selected before Rust handler code; user RSP/RIP
are canonical and user-range; direction flag and privileged flags are
sanitized; no arbitrary user pointer is dereferenced before ADR 0009's
recovery contract exists; no `sysret` target can fault into an unsafe privilege
transition.

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
user through checked slices (only after ADR 0015 fault-recovery is proven). No public arbitrary-frame mapper exists.

**Internal responsibilities:** own mapping metadata, page-table application,
TLB invalidation, memory-object lifetime, and, after ADR 0009, checked
fault-recoverable user-copy mechanics.
The physical allocator remains the sole source of page-table frames. `PhysicalMemory` and `UntypedMemory`
capabilities do not exist in v0.5; use anonymous `MemoryObject`s charged to a `ResourceDomain`.

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

**Recommended direction:** A shared trap/context contract across ADR 0008/0010 before M2 code.
Explicit thread states (`New`, `Runnable`, `Running`, `Blocked`, `Dying`, `Dead`), intrusive run and
wait queues. FPU/SIMD xstate must be explicitly forbidden or saved/restored.
wait queues owned by the scheduler, cooperative switching first, then APIC
timer preemption after Gate V5-D accepts calibration. v0.5 uses FIFO within a
small number of fixed priority bands or a simple fair queue selected by the
gate; it does not promise EEVDF.

**Owner modules:** `kernel-core::thread`, `kernel-core::scheduler`,
`kernel-core::time`, and `arch/x86_64/context.rs` / `apic.rs`.

**Public API shape:** create/start/yield/block/exit threads; wait/signal
notifications; scheduler-visible timer arm/cancel only through `TimerObject`.

**Internal responsibilities:** own task state transitions, run/wait queues,
context lifetime, timer-to-reschedule handoff, and reaping. The durable thread
context is distinct from the transient M2 trap frame, but both use a shared
documented register and kernel-stack contract. Scheduler policy does not own
IPC message contents or capability decisions.

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
modules: one trusted static init image and one read-only ramfs image. Extend
only the architecture entry layer to copy module descriptors into
`BootContext`. The kernel loads only init into a fresh address space; every
remaining module is exposed to init as opaque read-only `MemoryObject` data.
Init owns archive parsing, names, service discovery, and restart policy.

**Owner modules:** `boot.rs`, `kernel-core::boot_payload`,
`kernel-core::elf_loader`, `user/init`, and `xtask` image composition.

**Public API shape:** a private bounded manifest parser, static init-image
loader, and documented initial-capability manifest. Init receives a bounded
`ResourceDomain` Factory right, self/control authority, and a read-only ramfs
memory capability; a bootstrap console capability is optional. It receives no
physical-memory, page-table, APIC, raw I/O, or unrestricted debug authority.

**Internal responsibilities:** copy and validate boot descriptors, parse only
the bounded manifest and static init-image subset, construct init's initial
objects, and then relinquish archive parsing, service naming, and lifecycle
policy to init.

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

**Future extension points:** signed payload manifests, dynamic loading, and
richer restart supervision are deferred until their trust and resource
accounting requirements are separately accepted.

### 6.7 Developer console, ramfs, and shell

**Recommended direction:** A deterministic scripted shell (not mandatory interactive input).
ramfs and shell are user-space processes. The
kernel exports neither path parsing nor file semantics. v0.5 requires a
deterministic scripted developer session with capability-gated serial output;
it does not require interactive input. A real input path remains a later
port-I/O and IRQ-capability decision.

**Owner modules:** `user/ramfs`, `user/shell`, `user/init`; narrowly scoped
bootstrap console glue only if Gate V5-G accepts it.

**Public API shape:** ramfs request/reply protocol for open/read/list; shell
uses only service endpoint capabilities and no filesystem kernel syscall.

**Internal responsibilities:** init owns service routing, ramfs owns image
format and path semantics, and the shell owns command syntax and script
execution. Any bootstrap console glue owns only its narrowly granted output
bridge.

**Invariants:** shell input/output does not grant ambient I/O privilege; ramfs
paths and command parsing remain user-space; a malformed file request cannot
corrupt service memory; a console capability does not imply general port I/O.

**Alternatives considered:**

- Kernel-resident shell/ramfs is rejected as a direct violation of the future
  microkernel split.
- Immediate user-space PS/2 support requires a port-I/O and IRQ design that
  deserves its own ADR; it must not be smuggled in as shell plumbing.
- Interactive serial or PS/2 input is rejected for v0.5 because it would add
  a device-authority and interrupt-routing dependency to a service milestone.

**Future extension points:** a real device-capability model, interrupt-backed
input, writable filesystems, process launch policy, and richer shells remain
user-space work and must not enlarge the output-only console bridge.

## 7. Milestone Program

### M0: Pre-Code Architecture Authorization

**Objective:** Authorize required contracts before the first implementation commit.

**Dependencies:** v0.1 tags and exact evidence.

**Scope:**
1. A v0.5 requirements-trace document.
2. ADR 0007: Capability identity, delegation, and revocation.
3. ADR 0016: `ResourceDomain`, Factory authority, fallible kernel allocation, and amended object model.
4. ADR 0015: User-memory access and page-fault recovery.
5. Revised M2 split and M7 scripted-shell criterion.
6. V5-I (ADR 0017) scheduling for payload pipeline.
7. Test-scenario and CI-scaling design.

**Exit:** All documents are merged and accepted.

### M1: Object lifecycle and capability-space model

**Objective:** implement the accepted V5-A object arena and capability state
machine in host-testable `kernel-core`.

**Dependencies:** ADRs 0007 and 0008.

**Scope:** a safe host-testable `kernel-core`, a no_std `gaxera-abi`, fallible
typed object arenas, `ResourceDomain`, Factory rights, object IDs, capability
slots, type/rights checks, derivation, destroy, stale-handle detection,
transfer transactions, and revocation model. It does not integrate a kernel
object store, syscall, user mode, or hardware behavior.

**Unsafe invariants:** none are expected in M1. Semantic object identity is an
index/generation pair, not a storage address; slot reuse requires generation
change; accounting cannot underflow; exhaustion is represented as an error.

**Verification/evidence:** exhaustive state-table tests plus property tests for
derive/revoke/transfer sequences and allocation exhaustion. The unchanged
v0.1 UEFI matrix is the required guest regression proof.

**Exit:** every object lookup path has deterministic success and denial tests;
revocation is immediate for future lookups; every creation path is fallible;
no raw object pointer crosses an API boundary.

### M2A: Privilege transition and isolated address space proof

**Objective:** establish V5-B/V5-C and prove a fixed ring-3 probe can enter and
return from an isolated address space before a public syscall ABI exists.

**Dependencies:** M1, ADRs 0010 and 0011 (accepted).

**Scope:** user selectors, GDT/TSS/`RSP0` ownership, a per-thread kernel stack,
fixed user page tables, a minimal static code-page probe, and an internal-only
privilege-transition test path. No public syscall ABI, user pointer, or ELF
loader is introduced. The probe toolchain must not emit user extended-state
instructions until M3 establishes their save/restore contract.

**Unsafe invariants:** architecture code installs `RSP0` before any ring-3
entry; all user return state is canonical and user-range; kernel mappings stay
supervisor-only; TLB state matches mapping changes.

**Verification/evidence:** host range/permission tests; QEMU profiles for user
entry, denied privileged instruction, invalid user return state, and clean
return to the kernel.

**Exit:** an unprivileged probe cannot read/write kernel or HHDM addresses and
every privilege transition uses the intended GDT/TSS/kernel-stack contract.

### M2B: Syscall ABI and fault-recoverable user access

**Objective:** add the public syscall boundary only after privilege transition
and user-copy recovery contracts are independently accepted.

**Dependencies:** M2A, ADR 0009, and the syscall portion of ADR 0010.

**Scope:** versioned register-only syscall ABI, `syscall` entry, audited
`iretq`/`sysret` return policy, capability-authorized no-op/yield operations,
then bounded fault-recoverable user-copy routines. Explicit `MemoryObject`
mappings are the only bulk-data path.

**Unsafe invariants:** a fault resume is installed only for a known copy range,
cannot nest, and is cleared on every return; unrelated kernel page faults
remain terminal; `swapgs` and return state cannot cross a privilege boundary
with unchecked addresses or flags.

**Verification/evidence:** host ABI/range/fault-state tests; QEMU syscall round
trip, invalid handle, bad user pointer, bad return address, and user guard-page
profiles.

**Exit:** every public syscall is capability checked and a malicious user
pointer returns a defined error without corrupting or terminating the kernel.

### M3: Threads and cooperative execution

**Objective:** prove context ownership and task lifecycle before timer-driven
preemption.

**Dependencies:** M2B and ADR 0012's cooperative portion.

**Scope:** thread objects, kernel stacks, context save/restore, start/yield,
block/wake/exit transitions, one BSP run queue, an explicit shared
trap/context register contract, and deterministic two-task user proof. User
extended-state use remains prohibited until the context contract saves it.

**Unsafe invariants:** saved context contains all required state; a dead task
is never scheduled; no thread frees its current stack; queue links have one
owner.

**Verification/evidence:** host state-machine/property tests; QEMU alternation
markers from two isolated tasks and deliberate task exit/reap proof.

**Exit:** two user tasks can yield in a deterministic order without corrupting
kernel or each other's state.

### M4: Endpoint IPC, notifications, and capability transfer

**Objective:** implement V5-E mechanism without scheduler policy leakage.

**Dependencies:** M1 through M3 and ADR 0013.

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

### M5: Time source, timer object, and preemptive single-CPU scheduler (Complete)

**Objective:** turn the Phase 5 delivery proof into an accepted, bounded
preemption mechanism without claiming production timekeeping.

**Dependencies:** M3, M4, completed V5-D calibration decision.

**Scope:** selected monotonic tick source, APIC timer programming, minimal
timer object, reschedule request from IRQ, safe switch point, and simple fair
or priority-band policy accepted by ADR 0012.

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

**Dependencies:** M1 through M5 and ADR 0014.

**Scope:** copied Limine module descriptors in BootContext, bounded payload
manifest, static init-image loader, opaque read-only ramfs `MemoryObject`,
initial `ResourceDomain`/thread/address-space/capability space, init service
registry, and controlled child-service crash/restart proof.

**Unsafe invariants:** module boundaries are checked; ELF segments obey W^X;
init cannot obtain unrestricted kernel/system authority by handle forgery;
restart reclaims or invalidates all dead-service capabilities and mappings.

**Verification/evidence:** malformed module/ELF host tests; QEMU init-ready,
least-authority manifest, service crash, revocation, and restart markers.

**Exit:** the kernel loads only init from packaged data and knows no service
name, archive format, shell, or ramfs protocol after that handoff.

### M7: Ramfs service and developer shell

**Objective:** reach the v0.5 usable developer surface without moving file or
shell policy into the kernel.

**Dependencies:** M4 through M6 and ADRs 0015/0016.

**Scope:** read-only in-memory filesystem image, ramfs list/open/read protocol,
capability-routed service discovery, scripted shell command loop, `help`,
`ls`, `cat`, and `echo`, plus the accepted output-only developer-console path.

**Unsafe invariants:** console bridge does not grant ambient port access;
ramfs parser bounds checks every image offset; shell buffers remain user-space;
service IPC payload validation is service-owned.

**Verification/evidence:** ramfs parser host fuzz/property corpus; a QEMU
scripted session that lists and reads seeded files, capability denial to an
unrelated task, serial transcript output, and ramfs-service restart/reconnect
proof.

**Exit:** a deterministic user-space script reads a bundled file through ramfs
IPC and emits a verified serial transcript. The kernel contains neither
filesystem paths nor commands.

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

- privilege transition, user entry, and hostile-syscall return validation;
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
| `sysret`/`swapgs` error permits privilege confusion | This is a high-severity x86 boundary. | V5-B hostile-return audit, tiny assembly surface, QEMU negative tests, ADR 0010. |
| User-copy faults reenter an unsafe kernel state | Copying user bytes is a new faulting path. | V5-I recovery design, fault labels or checked mappings, QEMU bad-pointer tests. |
| User-triggered allocation panics the kernel | The fixed v0.1 heap has no process-level recovery policy. | V5-J `ResourceDomain`, fallible arenas, exhaustion tests, and no allocation in IRQ paths. |
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
- Establish bounded `ResourceDomain` accounting and fallible object allocation
  without claiming a global OOM policy.
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
8. A user-space ramfs service and deterministic scripted developer session can
   list and read seeded in-memory files through IPC. No kernel code parses
   paths, archives, or shell commands.
9. Host model/property/parser tests and the deterministic UEFI QEMU scenario
   matrix pass from a clean locked build. v0.1 profiles remain passing.
10. Exact-commit evidence, ADRs, ABI specs, workflow, environment, handoffs,
    governance records, and release CI are synchronized. Every temporary
    authority path is either removed or explicitly listed with a successor.

Meeting these criteria establishes an operating microkernel development system,
not a secure desktop or production OS. That distinction is part of the release
contract.
