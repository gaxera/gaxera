# Gaxera Foundation v0.1 Architecture

> **Status:** Canonical architectural reference
> **Release source:** [`v0.1.0`](../../) and `phase-6-complete`, both targeting
> `f6b2146efffd1c36c4e73d8006c7a20b3d89a3b9`
> **Last updated:** 2026-07-17

## 1. Purpose and Scope

Gaxera Foundation v0.1 is a verified x86-64 UEFI microkernel foundation. It is
not yet a general-purpose operating system. It proves the hardware and
ownership contracts on which later user-space, capability, IPC, and scheduling
work will depend.

The definitive source for the release is the annotated `v0.1.0` tag. This
document describes the architecture present at that source. Later evidence and
documentation commits register provenance; they do not alter the tagged kernel
implementation. When records disagree, precedence is: tagged source and exact
checkpoint evidence, accepted ADRs, canonical specifications, then historical
handoffs and internal context.

v0.1 proves all of the following under UEFI QEMU:

- Limine handoff to a higher-half Rust kernel.
- Deterministic COM1 serial diagnostics and a framebuffer gradient.
- Gaxera-owned GDT, TSS, IDT, and double-fault IST entry.
- A Gaxera-owned CR3 hierarchy with W^X/NX mappings and a RAM-only HHDM.
- Physical-frame allocation and a guarded kernel heap supporting `Box` and
  `Vec`.
- ACPI RSDP -> XSDT -> MADT discovery, a BSP xAPIC mapping, and deterministic
  periodic local-APIC timer delivery.
- Bounded allocation-free panic diagnostics with CPU state and raw
  frame-pointer backtrace output.

It does not provide processes, ring-3 execution, a scheduler, IPC,
capabilities, filesystems, persistent storage, drivers, SMP, timekeeping, or
hardware-grade device support.

## 2. Design Philosophy

Gaxera is intended to become a capability-based microkernel. The v0.1 kernel
therefore prefers narrow mechanism over early policy: it owns the minimum
machine state needed to make later isolation possible and does not absorb
driver, filesystem, display, or scheduling semantics prematurely.

The engineering philosophy is equally architectural:

- Evidence, not a successful compile, establishes a claim.
- Unsafe code must name its invariant at the boundary where it is used.
- Ownership must be explicit before state becomes shared or persistent.
- Stable interfaces remain small until a consumer proves they need expansion.
- UEFI QEMU is the required v0.1 development, CI, and release target.
- Reproducibility relies on an exact Rust nightly, committed `Cargo.lock`,
  exact critical crate versions, and a committed Limine SHA-256 reference.

These rules are formalized by ADRs 0000 through 0006 and the governance
constitution. The project's long-term capability and privacy goals are not
claimed as implemented security properties of v0.1.

## 3. System Shape

```text
UEFI firmware
  -> Limine v12.4.2
    -> _start assembly trampoline
      -> Gaxera bootstrap stack
        -> descriptor and IDT setup
          -> BootContext capture
            -> bootstrap frame allocation and page-table construction
              -> Gaxera CR3 activation
                -> bitmap frame allocator and guarded heap
                  -> framebuffer proof, ACPI discovery, Local APIC setup
                    -> interrupt-enabled idle or deterministic QEMU profile
```

The host-side `xtask` crate builds the ELF and hybrid ISO, launches QEMU, and
interprets deterministic guest success. The kernel is `#![no_std]` and uses
only `core`, `alloc`, exact-pinned `x86_64`, exact-pinned
`linked_list_allocator`, and the Limine protocol crate. No host runtime or C
runtime executes in the kernel.

## 4. Boot Architecture

### 4.1 Image and entry

`kernel/src/arch/x86_64/linker.ld` places the kernel at
`0xffffffff80000000` and emits page-aligned R-X text, R-- rodata, and RW data
segments. The Limine request records occupy their own writable `requests`
segment because Limine writes response pointers there during handoff. After
Gaxera takes ownership of paging, request metadata is mapped read-only and NX.

`kernel/src/arch/x86_64/entry.rs` provides `_start`. It replaces Limine's stack
before Rust code runs with a 64 KiB statically allocated bootstrap stack. A
lower guard page is left unmapped after the CR3 transition. The size is a
measured debug-build requirement, not an arbitrary default.

### 4.2 Limine containment

`kernel/src/arch/x86_64/boot.rs` is the only production module that reads
Limine request-response types. It validates and copies the required handoff
data into `BootContext`, published as immutable Gaxera-owned metadata:

- normalized memory regions;
- kernel image physical and virtual bases;
- validated framebuffer metadata, if present;
- RSDP physical metadata, if present;
- the pre-transition HHDM offset required only while constructing Gaxera's
  first page-table hierarchy.

No allocator, framebuffer, paging consumer, ACPI parser, or main boot path
retains a Limine response pointer. Later subsystems consume `&'static
BootContext`, never bootloader-owned protocol data.

### 4.3 Build and packaging boundary

`xtask/src/main.rs` owns bootstrap, build, ISO packaging, QEMU invocation, and
test orchestration. `cargo xtask bootstrap` downloads Limine v12.4.2 only when
needed, checks its SHA-256 against a committed source value, repairs a partial
cache by re-extracting it, builds its host tool, and stages the artifacts.

`cargo xtask build` produces `target/gaxera.iso`. The image retains BIOS
packaging support for diagnostics, but UEFI is the supported architecture and
the only release/CI validation target.

## 5. Ownership Model and Subsystem Boundaries

| Owner | Responsibility | Boundary it enforces |
| --- | --- | --- |
| `boot.rs` | Limine negotiation and copied handoff capture | Protocol types and response pointers do not escape. |
| `memory/boot.rs` | `BootContext`, memory classification, reservation input | Firmware data becomes immutable Gaxera metadata before allocation. |
| `memory/physical.rs` | bootstrap and bitmap physical-frame allocation | Usable frames are allocated once; boot/kernel reservations remain unavailable. |
| `paging.rs` | Gaxera page-table construction, CR3, typed mappings | No general mapper is exposed to arbitrary callers. |
| `memory/heap.rs` | guarded fixed heap and global allocator initialization | Heap memory is mapped RW+NX before allocator publication. |
| `descriptors.rs` and `exceptions.rs` | GDT, TSS, IDT, terminal exception policy | Double faults use a dedicated static IST stack. |
| `acpi.rs` | bounded copied-table RSDP/XSDT/MADT parsing | Firmware-table physical pointers and mappings do not persist. |
| `apic.rs` | BSP xAPIC MMIO, PIC masking, timer, EOI | Timer handler has no allocation, logging, locks, or scheduler semantics. |
| `diagnostics.rs` | bounded panic CPU state and backtrace | Panic telemetry cannot scan arbitrary memory or allocate. |
| `serial.rs` and `framebuffer.rs` | early diagnostics and visual proof | COM1 is a development console; framebuffer support is intentionally narrow. |
| `xtask` | reproducible host orchestration and QEMU result validation | Guest success requires expected serial evidence and `isa-debug-exit`. |

This is deliberately not a general kernel object model yet. There is no public
kernel ABI. Future code must not treat these bootstrap owners as a substitute
for address-space, thread, capability, endpoint, notification, or timer
objects.

## 6. Memory Architecture

### 6.1 Physical memory

The boot context classifies every Limine memory-map range and emits a
deterministic serial dump before allocator initialization. Only ranges marked
usable are allocator eligible. Kernel image frames, bootstrap and IST stacks,
page-table frames, allocator bitmap backing, framebuffer memory, bootloader
memory, ACPI memory, and reserved regions are not handed out accidentally.

Bootstrap allocation uses a checked range allocator while Limine's HHDM is
still available. Once Gaxera activates its own tables, a segmented bitmap
allocator becomes the long-lived physical allocator. Its bitmap backing is
allocated and permanently reserved before the global allocator is published.

### 6.2 Virtual memory and CR3

`KernelPageTables::build` constructs a new four-level hierarchy before
switching CR3. It maps only selected ranges:

- usable RAM through `HHDM_BASE`, never arbitrary physical addresses;
- kernel text R-X, rodata and request data R+NX, data/BSS/stacks RW+NX;
- the validated framebuffer at a dedicated virtual address;
- a fixed 2 MiB heap RW+NX between two unmapped guard pages;
- one temporary read-only NX ACPI page window;
- one permanent Local APIC page window with PWT+PCD cache attributes.

Before loading the new root, `paging.rs` enables CR0 write protection and EFER
NXE. The transition continuity set includes executing code, the active
bootstrap stack, descriptor state, static data, and immediately used mappings.
Interrupts remain disabled while this state is constructed. PCID is rejected;
the CR3 write flushes non-global translations.

### 6.3 Heap

The first heap page is translated through the new tables before
`linked_list_allocator` is initialized. The heap is fixed-size and allocated
from frames selected by Gaxera, not Limine. The lower guard proof deliberately
accesses the unmapped guard page and requires the page-fault handler to report
the exact CR2 address.

There is no demand paging, page-table reclamation, huge-page policy, per-task
address space, copy-on-write, swap, memory pressure handling, or concurrent
allocator contract in v0.1.

## 7. Exception and Descriptor Architecture

`descriptors.rs` owns static GDT and TSS storage and a 32 KiB double-fault IST
stack. It loads code/data segments and TSS once on the bootstrap processor.
`exceptions.rs` initializes a static IDT once while interrupts are disabled.

The installed policy is:

- breakpoint is resumable and reports its instruction pointer;
- divide error, invalid opcode, general protection fault, and ordinary page
  fault are terminal in v0.1 test/diagnostic paths;
- double fault uses IST index 0 and succeeds only after its handler proves RSP
  lies inside the dedicated stack;
- timer vector `0xe0` is delegated to the Local APIC owner;
- spurious vector `0xff` intentionally does no work and does not issue EOI.

The double-fault test is a real processor escalation: its test image leaves
the page-fault gate non-present, triggers a page fault, and lets exception
delivery escalate to double fault. It is not an `int 8` shortcut.

## 8. Firmware Discovery and Interrupt Architecture

### 8.1 ACPI

The minimal ACPI parser supports only the release need: ACPI revision 2+ RSDP,
XSDT, SDT headers, and MADT Local APIC information. It validates signatures,
declared lengths, checksums, XSDT alignment, table bounds, and MADT subtable
lengths. A valid MADT Local APIC Address Override supersedes the MADT header.

Physical table access is page-at-a-time. `PagingReader` maps one
ACPI-reclaimable page at the fixed temporary window, copies requested bytes,
drops the derived view, unmaps the page, and flushes its TLB entry before the
next access. The kernel asserts the window is absent after discovery.

### 8.2 Local APIC

The implementation supports only the bootstrap processor in xAPIC MMIO mode.
It validates CPUID APIC/PAT support, BSP state, inactive x2APIC mode, the
MADT-selected physical address against IA32_APIC_BASE, and the PAT entries used
for the firmware and APIC mappings. It maps the APIC page only after rejecting
an unaligned address, usable-RAM alias, or framebuffer alias.

The legacy 8259 PICs are masked. The APIC timer is configured for a
delivery-only periodic proof at vector `0xe0`. Its handler increments an atomic
counter, masks the timer exactly at the target, publishes completion, then
sends EOI. Production boot enables interrupts only after this setup and idles
with HLT.

This is not a clock, scheduler tick, IRQ routing layer, general MMIO service,
or SMP foundation. x2APIC, IOAPIC, MSI, AML, timer calibration, clocksource
selection, and external device interrupts remain absent.

## 9. Diagnostics Model

Serial COM1 is available from first Rust entry. It writes directly without
transmit-status polling, an intentional QEMU-first choice that is unsuitable
as a claim of robust physical UART support. The framebuffer renderer accepts
only the validated simple 32-bit linear layout required for the QEMU proof.

The panic handler prints source location and message, then invokes
`diagnostics.rs`. Every bare-metal build forces frame pointers. Diagnostics
records `RSP`, `RBP`, `RFLAGS`, `CR2`, and the active CR3 root, then walks at
most 16 return addresses. A frame is read only when both words are aligned and
inside the bootstrap or double-fault IST stack; links must increase
monotonically. It reports a stop reason rather than scanning arbitrary memory.

The values describe panic-handler context, not an immutable fault frame. `CR2`
may be stale for non-page-fault panics. Raw addresses are intentionally not
symbolized because the runtime discards unwind metadata and contains no trusted
symbolizer.

## 10. Verification and Reproducibility

`cargo xtask test` is the release verification entry point and is run by CI.
It performs locked target checking, host kernel unit tests, strict Clippy for
the normal profile plus all feature-gated guest profiles, and deterministic
headless UEFI QEMU profiles. Every guest profile requires its expected serial
markers and, except the normal boot run, the expected `isa-debug-exit` status.

The exact v0.1 evidence at
`docs/evidence/checkpoint-06/2026-07-17_phase6_commit-f6b2146_verification.log`
records:

- 11 host unit tests;
- strict Clippy for 12 kernel profiles;
- 11 UEFI guest-confirmed profiles: normal boot, panic, memory, heap guard,
  APIC timer, breakpoint, divide error, invalid opcode, general protection,
  page fault, and processor-escalated double fault;
- normal ISO restoration after test-only builds.

The CI workflow calls the same `cargo xtask test` command after formatting,
host linting, dependency installation, and Limine bootstrap. This establishes
reproducibility for the documented QEMU/OVMF environment, not a claim of
physical-hardware certification or hermetic system-package inputs.

## 11. Stable Architectural Invariants

Future work may rely on these invariants unless a superseding ADR explicitly
changes them:

1. `BootContext` is the only post-handoff source of boot metadata, and Limine
   response pointers do not escape `boot.rs`.
2. Physical-frame ownership is unique. Allocator-eligible means usable memory
   only, after all kernel and bootstrap reservations are excluded.
3. `KernelPageTables` is the sole page-table mutation owner. Mapping APIs are
   typed and policy constrained; arbitrary physical mapping is not available.
4. Kernel mappings preserve W^X/NX and write protection. Guard pages remain
   genuinely unmapped.
5. The HHDM covers usable RAM only. Firmware and MMIO use deliberately
   reviewed, non-aliasing mapping paths.
6. Descriptor and IDT state initializes once before interrupts are enabled;
   double faults enter a dedicated static IST stack.
7. The Local APIC is BSP-only xAPIC MMIO. The timer handler remains
   allocation-free, non-blocking, and free of scheduler policy.
8. Panic telemetry remains bounded, allocation-free, and restricted to owned
   static stacks.
9. Deterministic guest claims require both serial evidence and guest-confirmed
   exit, and test images never remain packaged as the normal ISO.

## 12. Security Position

The v0.1 foundation reduces accidental kernel corruption through Rust,
ownership boundaries, W^X/NX, guarded stacks/heap, and constrained unsafe
interfaces. It does not yet enforce the future system's capability security
model because there are no untrusted tasks, capabilities, syscalls, device
drivers, DMA policy, authenticated boot chain, or persistent user data.

Security properties that are true now:

- no bootloader pointer is used after Gaxera copies handoff metadata;
- untrusted physical ranges are not arbitrarily mapped through the HHDM;
- kernel code and read-only data are protected after CR3 activation;
- fatal exception and panic paths halt rather than resume in unknown state;
- malformed ACPI data is bounds and checksum checked before use.

Security properties intentionally not claimed now include user isolation,
capability confinement, ASLR/KASLR, secure boot, IOMMU/DMA isolation, cryptic
key handling, persistent crash protection, or physical-device robustness.

## 13. Known Technical Debt and Intentional Deferrals

The following are known limits, not silent defects:

- COM1 output does not poll transmitter readiness and is QEMU-oriented.
- Framebuffer support is a proof path, not a graphics driver.
- Empty intermediate page-table nodes created by temporary mappings are not
  reclaimed.
- The heap is fixed at 2 MiB with no concurrent allocation policy.
- ACPI parsing excludes ACPI NVS/reserved access, AML, root-table variants
  beyond XSDT, and general table services.
- The APIC timer is uncalibrated and supports no timekeeping, scheduling,
  external IRQ routing, or SMP.
- No physical hardware, multiple firmware, or non-QEMU device validation has
  been performed.
- The build environment is pinned at the Rust/dependency/bootloader layer but
  still depends on mutable host and CI package repositories.
- Kernel and user-space ASLR/KASLR, formal verification, and secure boot are
  future security work.

## 14. Guidance for Post-v0.1 Engineers

Do not widen a bootstrap API merely because a new subsystem needs memory,
interrupts, or firmware access. First decide the new object owner, caller
rights, lifetime, failure behavior, and test proof. Record architecture-shaping
decisions in an ADR before implementation.

Do not interpret the periodic APIC proof as a scheduler clock. Do not turn the
RAM-only HHDM into a general MMIO shortcut. Do not use Limine structures after
`BootContext` capture. Do not add locks, concurrent allocation, SMP, user-mode
entry, or interrupt routing without a per-CPU and lifecycle design.

The right next step is not to grow every subsystem at once. Establish the
resource and object/capability model, privilege-transition proof, syscall and
user-copy boundary, controlled address-space mappings, scheduler/time
ownership, IPC semantics, and service bootstrap in dependency order. The
frozen v0.5 engineering program and its requirements trace record that work;
they do not alter this immutable foundation contract.
