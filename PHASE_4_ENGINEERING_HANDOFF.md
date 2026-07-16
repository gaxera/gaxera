# Phase 4 Engineering Handoff: Physical and Virtual Memory Foundations

> Status: Complete pending release provenance registration.
> Scope: Phase 4 from the approved implementation plan through the closeout audit.
> Required execution target: UEFI QEMU/OVMF. BIOS remains a packaging diagnostic only.

## 1. Executive Record

Phase 4 transfers early-memory ownership from Limine to Gaxera. The kernel now
captures boot metadata into one immutable Gaxera-owned `BootContext`, emits a
stable classified memory-map diagnostic, allocates frames without using
bootloader-reclaimable memory, constructs its own four-level page tables,
activates them through CR3, and bootstraps a fixed guarded kernel heap.

This is deliberately not an address-space subsystem for processes. It is the
minimal, reviewed substrate needed for later ACPI/APIC discovery, kernel
objects, and eventually separate address spaces. The permanent policy is:

- only `MEMMAP_USABLE` is allocator eligible;
- Gaxera's HHDM maps usable RAM only, not arbitrary physical addresses;
- MMIO and framebuffer mappings are explicit and separate;
- text is RX, read-only data and request metadata are R+NX, writable data and
  stacks are RW+NX, and heap pages are RW+NX;
- CR0.WP and EFER.NXE are enabled before the owned mapping is activated;
- the bootstrap, IST, and heap lower guard pages are deliberately unmapped;
- no post-handoff subsystem reads a Limine response structure.

The closeout audit found and corrected four material implementation problems:

1. A 32 KiB bootstrap stack double-faulted under debug builds because large
   fixed-size allocator construction frames exceeded the stack. The static
   stack is now 64 KiB and retains an unmapped lower guard.
2. The first mapper API was generic enough to map arbitrary virtual pages.
   It was narrowed to the checked `map_heap_page` operation. Arbitrary MMIO
   mapping is intentionally deferred to a reviewed Phase 5 API.
3. The Limine `.requests` page was initially remapped RW+NX after CR3. It is
   now separately R+NX: Limine writes it before handoff, and Gaxera only reads
   it while copying the handoff.
4. RSDP conversion initially assumed a virtual HHDM pointer. Limine specifies
   a physical RSDP address at base revision 3 and a virtual address at other
   revisions. The capture path now branches on the negotiated revision before
   publishing the copied physical address.

## 2. Plan and Research Gates

The approved Phase 4 plan established three gates before implementation.

### 2.1 Research Gate 4A: Boot context and physical allocation

**Decision:** ADR 0003 selects a bounded immutable `BootContext`, a small
bootstrap range allocator, then a segmented bitmap allocator. `BootReservations`
records bootstrap/page-table/bitmap frames so the steady-state allocator never
returns them.

**Why:** Passing Limine structures through the kernel would extend a
bootloader-owned lifetime past CR3. A dense bitmap from physical zero would
scale with address-space holes; the segmented bitmap scales with usable frames.
Only usable memory is safe to allocate before ownership policies for ACPI,
bootloader reclaimable, framebuffer, and module memory exist.

### 2.2 Research Gate 4B: Paging and CR3 ownership

**Decision:** ADR 0004 selects exact four-level Limine entry, the exact-pinned
`x86_64 = "=0.15.5"` crate's `MappedPageTable`, Gaxera-owned tables, a
RAM-only HHDM, and a complete continuity mapping before CR3.

**Why:** `OffsetPageTable` asserts that every physical address is permanently
available at one virtual offset. That is incompatible with the RAM-only HHDM
and future MMIO cache policy. `MappedPageTable` delegates page-table mechanics
while Gaxera retains frame lifetime, permissions, cache policy, TLB flushing,
and CR3 ownership.

### 2.3 Research Gate 4C: Heap bootstrap

**Decision:** ADR 0004 selects exact-pinned
`linked_list_allocator = "=0.10.6"` with `use_spin`, backed by a fixed 2 MiB
heap and guard pages.

**Why:** A bump allocator would not exercise normal allocation/deallocation
behaviour required by `Box` and `Vec`. A fixed initial heap proves `alloc`
without prematurely implementing growth, demand paging, or a general virtual
memory manager.

## 3. Final Architecture and Ownership Boundaries

### 3.1 Strict boot boundary

`kernel/src/arch/x86_64/boot.rs` is the sole production module importing
`limine` types or reading Limine response pointers. It owns all request statics,
verifies the base revision and exact paging mode, classifies descriptors, and
copies only selected data into `BootContext`.

`BootContext` contains copied region records, kernel physical/virtual bases,
validated framebuffer metadata, and an optional physical RSDP address. It
contains no Limine slice, response pointer, or Limine type. Its construction
path is crate-private and called only by the architecture handoff module. The
binary entry receives `BootHandoff`, exposes only `&'static BootContext` plus a
temporary pre-CR3 HHDM offset, and never imports Limine.

Limine request records remain static protocol data because the boot protocol
requires them. Their private response fields cannot be erased through the
crate API, but no Gaxera subsystem receives a reference to them. After CR3 they
are mapped R+NX, and no code reads them after `capture_handoff` returns.

### 3.2 Physical ownership

`BootstrapFrameAllocator` walks sorted usable ranges and records every returned
frame in `BootReservations`. Page-table frames and contiguous bitmap backing
are therefore reserved before `SegmentedBitmapFrameAllocator` is initialized.
The bitmap initially marks every bit used, then frees exactly usable,
non-reserved frames. It does not allocate bootloader reclaimable, ACPI,
framebuffer, executable/module, mapped-reserved, bad, reserved, or unknown
memory.

The allocator is intentionally bootstrap-CPU-only. Its global storage and
mutable reference rely on interrupts remaining disabled and no SMP execution.
Phase 5 must not introduce concurrent allocation; an explicit synchronized
allocator interface is required before that becomes legal.

### 3.3 Virtual layout and CR3

The permanent Phase 4 layout is defined in `kernel/src/memory/mapping.rs`:

| Range | Policy |
| --- | --- |
| `0xffff800000000000` HHDM | Usable RAM only, RW+NX |
| `0xfffffe0000000000` framebuffer window | Explicit framebuffer mapping, RW+NX+NO_CACHE |
| `0xfffffe8000000000` | Lower heap guard, unmapped |
| `0xfffffe8000001000` through 2 MiB | Kernel heap, RW+NX |
| Heap upper adjacent page | Unmapped |
| `0xffffffff80000000` | Statically linked kernel image with segment-specific permissions |

Before `Cr3::write`, Gaxera maps executable code, read-only data, request
metadata, writable data/BSS, the bootstrap stack, IST stack, framebuffer, and
the RAM-only HHDM. The static stack guard gaps are left absent. PCID is rejected
for this phase, CR0.WP is set, EFER.NXE is set and read back, and the CR3 reload
flushes non-global translations.

## 4. Important Files

| Path | State | Purpose and important decisions |
| --- | --- | --- |
| `Cargo.lock` | Modified | Locks `x86_64`, `linked_list_allocator`, and transitive inputs for deterministic Cargo resolution. |
| `kernel/Cargo.toml` | Modified | Adds exact `x86_64 = "=0.15.5"` and `linked_list_allocator = "=0.10.6"`; adds memory and heap-guard test features. |
| `kernel/src/lib.rs` | Created | Shared `no_std` library exposing architecture, memory, serial, and framebuffer code to the kernel binary and host tests. |
| `kernel/src/main.rs` | Modified | Owns boot sequencing: descriptors, architecture handoff, diagnostic dump, bootstrap allocation, owned CR3, bitmap allocator, heap mapping, allocation proof, and test-profile exits. It contains no Limine type or response access. |
| `kernel/src/arch/x86_64/boot.rs` | Created | Sole Limine boundary. Defines request statics, validates base/paging revisions, converts revision-specific RSDP addressing, validates framebuffer metadata, and publishes immutable copied boot data. |
| `kernel/src/arch/x86_64/entry.rs` | Created | Supplies the 64 KiB static bootstrap stack and assembly trampoline that changes stacks before Rust entry. |
| `kernel/src/arch/x86_64/paging.rs` | Created | Builds inactive owned tables, maps the continuity set, applies W^X/NX policy, activates CR3, restricts post-transition mapping to fixed heap pages, and exposes explicitly unsafe translation. |
| `kernel/src/arch/x86_64/linker.ld` | Modified | Adds page-aligned `.requests`, stack sections, and lower guard gaps; exports mapping symbols used by the owned mapper. |
| `kernel/src/arch/x86_64/mod.rs` | Modified | Exposes the Phase 4 architecture modules. |
| `kernel/src/arch/x86_64/descriptors.rs` | Modified | Keeps descriptor/TSS/IST state compatible with the owned mapping and static guard layout. |
| `kernel/src/arch/x86_64/exceptions.rs` | Modified | Adds the heap-guard page-fault profile, verifies CR2 equals the lower guard, and preserves terminal QEMU test semantics. |
| `kernel/src/memory/mod.rs` | Created | Owns the boot, physical, mapping, and target-only heap modules. |
| `kernel/src/memory/boot.rs` | Created | Defines protocol-neutral `BootContext`, region classification, deterministic map dump, one-time publication, and crate-private construction primitives. `source_type` is diagnostic-only opaque metadata, not a Limine API. |
| `kernel/src/memory/physical.rs` | Created | Implements reservations, bootstrap allocation, segmented bitmap allocation, static allocator installation, and host unit tests. |
| `kernel/src/memory/mapping.rs` | Created | Defines the small fixed Phase 4 address-space constants. |
| `kernel/src/memory/heap.rs` | Created | Installs the one-time global `LockedHeap` only after validating aligned, non-wrapping mapped bounds. |
| `kernel/src/framebuffer.rs` | Modified | Consumes copied framebuffer metadata and the Gaxera-owned virtual mapping rather than a Limine response. |
| `xtask/src/main.rs` | Modified | Adds `memory` and `heap-guard` profiles to strict profile linting and deterministic UEFI QEMU execution. |
| `.github/workflows/ci.yml` | Modified | Runs the same locked `cargo xtask test` matrix in CI and is configured for the Phase 4 closeout tag. |
| `docs/adr/0003-boot-context-and-physical-memory-ownership.md` | Created | Accepted ownership and allocator decision. |
| `docs/adr/0004-kernel-address-space-transition-and-heap.md` | Created | Accepted paging, CR3, permissions, and heap decision. |

## 5. Unsafe Invariants

All architecture-critical unsafe code has a local `SAFETY` explanation. The
material invariants are:

1. The entry trampoline alone changes RSP to the aligned static bootstrap
   stack; the lower guard is unmapped only after Gaxera owns CR3.
2. Boot-context static storage is written once on the bootstrap CPU with
   interrupts disabled, then published only as `&'static BootContext`.
3. Bootstrap frames are unique, allocator-eligible, and reachable through the
   temporary Limine HHDM until CR3. Every table frame is zeroed before use.
4. The `PageTableFrameMapping` offset maps every allocator-returned table frame.
   Build-time HHDM address arithmetic is checked; after activation, all table
   frames are below Gaxera's bounded RAM HHDM limit.
5. CR3 changes only after the active code, static data, stacks, IDT/GDT/TSS,
   page tables, and framebuffer continuity paths are mapped. Interrupts remain
   disabled throughout Phase 4 bootstrap.
6. `translate` is unsafe because the upstream mapper requires `&mut PageTable`
   even for a read. Callers must exclude concurrent page-table mutation.
7. The global physical allocator and heap initialize once, on one CPU, before
   concurrent allocation. Their backing storage remains permanently reserved.
8. The heap guard test performs one volatile read of an intentionally unmapped
   address and succeeds only if the page-fault handler observes that exact CR2.

## 6. Verification and Evidence

### Automated local checks

`cargo xtask test` is the complete deterministic matrix. It performs locked
target checking, host-testable kernel unit tests, strict Clippy for the normal
image and every guest profile, then UEFI QEMU proofs for normal boot, panic telemetry, memory foundation,
heap guard, breakpoint, divide error, invalid opcode, general protection,
page fault, and processor-escalated double fault. Test-only images exit via
`isa-debug-exit`; `-no-reboot`, a 20-second timeout, expected serial markers,
and an expected guest exit code prevent triple faults, timeouts, or host-only
success from being misreported. The runner restores a normal ISO afterwards.

The Phase 4 profiles prove:

- `memory`: Gaxera-owned CR3 is active, the segmented allocator is initialized,
  a `Box` and a `Vec` allocate/read/deallocate successfully, and the first heap
  virtual page translates to its allocated physical frame.
- `heap-guard`: the same setup succeeds, then an access to the lower guard
  produces `GAXERA: HEAP_GUARD_PAGE_FAULT_CAUGHT` only when CR2 is exactly the
  expected unmapped address.

The host unit tests cover deterministic sort/classification and bitmap
reservation/deallocation behavior and are part of both `cargo xtask test` and
CI. They do not claim to validate privileged CPU state; that requires the UEFI
QEMU proofs above.

### Manual and diagnostic validation

The implementation was repeatedly built and run with:

```text
cargo fmt --check
cargo clippy --locked -p kernel --lib --target x86_64-unknown-none -- -D warnings
cargo test --locked -p kernel --lib
cargo xtask run -- --headless --test memory
cargo xtask run -- --headless --test heap-guard
cargo xtask test
```

The directed memory and guard runs produced the boot-context marker, a complete
classified map, `GAXERA: CR3_GAXERA_OWNED`,
`GAXERA: MEMORY_FOUNDATION_OK`, and, for the guard profile, the exact CR2
marker. Timestamped developer diagnostics are generated under ignored
`logs/qemu-*.log`; they are useful local debugging output, not immutable
repository evidence. The final exact-commit transcript is registered under
`docs/evidence/checkpoint-04/` during release closeout.

## 7. Problems Encountered and Resolutions

| Problem | Diagnosis | Final resolution |
| --- | --- | --- |
| Early post-CR3 boot reached #DF during bitmap setup | Disassembly showed nested debug-build frames of roughly 12 KiB in entry and 11 KiB in allocator construction, exceeding the 32 KiB bootstrap stack. | Increase static bootstrap stack to 64 KiB; retain unmapped lower guard; rerun UEFI memory and complete matrix. |
| Generic mapping API allowed policy bypass | A caller could request arbitrary virtual mappings and flags without Phase 4 MMIO/alias review. | Make generic mapper private; expose `map_heap_page` with fixed range and RW+NX flags only. |
| `.requests` writable after ownership transfer | It was mapped together with data for convenience, despite no permitted mutation after capture. | Map request metadata separately R+NX; map data/BSS/stacks RW+NX. |
| RSDP was captured with a single address interpretation | Pinned Limine binding documents base revision 3 as physical and all other revisions as virtual. | Read negotiated revision and preserve a physical RSDP value with the correct conversion. |
| Initial boot capture API exposed Limine-coupled construction | `BootContext::capture` took Limine types from a general memory module. | Move all request statics and parsing to architecture boot code; expose only immutable copied context. |

## 8. Intentional Limits and Phase 5 Constraints

These are deliberate Phase 4 boundaries, not defects:

- no SMP, preemption, `sti`, or synchronized allocator interface;
- no user address spaces, page-table reclamation, unmapping API, recursive map,
  global pages, PCID, huge pages, demand paging, or KASLR;
- no general MMIO mapper or APIC access;
- no reclamation of bootloader-reclaimable or ACPI reclaimable ranges;
- no physical-hardware claim; validation is QEMU 10.2.1 plus OVMF;
- no heap growth, OOM policy, or allocator use from exception handlers.

Phase 5 must preserve the immutable `BootContext` boundary, consume only its
physical RSDP value, define an explicit uncacheable MMIO mapping operation,
avoid HHDM cache-attribute aliases, and settle ACPI table validation and APIC
timer calibration before touching APIC registers.

## 9. Completion Argument

Checkpoint 4 required parsed usable RAM, dynamic allocation, correct virtual
translation, owned CR3, and CI verification. The implementation provides each:

| Exit criterion | Concrete evidence |
| --- | --- |
| Classified boot memory input before allocation | `BootContext::dump_memory_map` emits deterministic ordered `MEMMAP_*` records before bootstrap allocation. |
| Safe physical-frame ownership | `BootReservations`, bootstrap allocation, segmented bitmap initialization, and host allocator tests. |
| Kernel-owned page tables and CR3 | `KernelPageTables::build` plus `activate`; guest marker `GAXERA: CR3_GAXERA_OWNED`. |
| W^X/NX and guard-page policy | Segment-specific mapper calls, CR0.WP/EFER.NXE activation, omitted stack and heap guard pages, and a guest CR2 guard proof. |
| Heap works after CR3 | `Box`/`Vec` validation and `GAXERA: MEMORY_FOUNDATION_OK`. |
| Reproducible expanded verification | Locked `cargo xtask test`, CI invocation of that exact command, deterministic guest exits, and final Checkpoint 4 transcript. |
| No continued Limine dependency after handoff | Limine imports confined to `arch/x86_64/boot.rs`; all consumers use `&'static BootContext`. |

With release evidence registered against the implementation commit and CI green
for the annotated closeout tag, Phase 4 is complete and the repository is ready
to open Research Gate 5A.
