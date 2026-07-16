# ADR 0004: Kernel Address-Space Transition and Heap

## Status

Accepted

## Context

Phase 4 replaces Limine's temporary page tables while the kernel, its static
descriptor state, its stacks, and the framebuffer must remain executable or
accessible. The existing architecture uses `x86_64` paging types, which are
four-level abstractions, while the Limine paging mode is otherwise not an
explicit kernel contract.

The Limine HHDM is a bootloader mapping for selected ranges. It is not a
license to dereference arbitrary physical addresses and it must not dictate
Gaxera's permanent cache or MMIO policy.

## Decision

Gaxera requests exact Limine x86-64 four-level paging and verifies that
response during boot capture. Before enforcing protected mappings, it verifies
or enables CR0.WP and EFER.NXE. The Phase 4 mapper uses the pinned `x86_64`
crate's `MappedPageTable` with a Gaxera-owned physical-frame mapping proof.
It does not use `OffsetPageTable`, whose safety contract requires the complete
physical address space at one virtual offset.

The Phase 4 virtual layout reserves a RAM-only HHDM beginning at
`0xffff800000000000`, an unmapped MMIO/framebuffer window, a guarded kernel
heap region, a reserved kernel-dynamic range, and the existing kernel-image
slot. The HHDM maps only validated usable RAM. Framebuffer memory is mapped
separately. Phase 4 has no arbitrary MMIO mapping API, no global mappings, no
PCID, no recursive page-table mapping, no huge pages, and no user mappings.

The kernel moves from Limine's stack to a static Gaxera bootstrap stack before
Rust depends on the handoff. The bootstrap and double-fault stacks gain lower
guard pages when Gaxera activates its own tables. The transition maps kernel
text RX, read-only data R+NX, Limine request metadata R+NX after handoff,
writable kernel data RW+NX, tables RW+NX, and heap pages RW+NX. CR3 activation
occurs with interrupts disabled and only after a complete continuity audit.

The initial heap is a fixed, guarded mapping. Phase 4 uses
`linked_list_allocator = "=0.10.6"` after reviewing its `LockedHeap` API and
dependency surface. The allocator initializes once after the heap is mapped;
exception handlers and early telemetry must not allocate.

KASLR remains deferred. The static linker VMA is retained until Gaxera has an
explicit relocation and symbol/debugging strategy.

## Consequences

Page-table mechanics come from a reviewed crate while Gaxera retains control
of lifetime, mapping, cache, permission, TLB, and CR3 policy. Mapping only RAM
through the HHDM prevents an accidental write-back alias for future APIC MMIO.
The transition requires a small assembly entry trampoline and linker support
for stack sections, but removes the bootloader stack as a permanent kernel
dependency.

The initial heap enables ordinary Rust allocation without prematurely
implementing demand paging, user address spaces, or a full memory-pressure
policy.

## Alternatives Considered

Using Limine's HHDM as Gaxera's permanent physical-memory API was rejected
because its base and covered ranges are bootloader-defined. Mapping all memory
or enabling huge pages immediately was rejected because it would enlarge cache
aliasing, splitting, and guard-page obligations before they are needed.

A bump allocator was rejected as the default Phase 4 endpoint because it does
not exercise ordinary deallocation behavior needed by `Box` and `Vec`.
