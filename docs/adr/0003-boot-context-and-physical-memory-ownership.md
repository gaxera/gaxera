# ADR 0003: Boot Context and Physical Memory Ownership

## Status

Accepted

## Context

Limine provides request responses backed by bootloader-owned structures and
temporary page tables. Phase 4 needs to replace those tables, allocate frames,
and later retire all reliance on transient bootloader state. Passing Limine
types through the kernel would make that ownership boundary implicit and make
later reclamation unsafe.

Physical-frame allocation also has a bootstrap problem: bitmap metadata and
the page-table frames needed to map it require frames before a general frame
allocator exists. Firmware memory maps can be sparse and contain ranges that
are not allocator candidates.

## Decision

Gaxera captures a single immutable, Gaxera-owned `BootContext` before changing
CR3. It contains copied and validated memory-region records, framebuffer
metadata, kernel image physical/virtual metadata, and an optional physical
RSDP address. Capture accepts only the confirmed four-level paging mode, but
that protocol fact is not retained as a general kernel dependency. It contains
no Limine response pointer, slice, or type. The temporary Limine HHDM offset
is private to early capture and page-table construction and is not part of the
post-handoff kernel interface.

Only `kernel/src/arch/x86_64/boot.rs` may construct the production context or
read Limine requests and responses. All later code consumes immutable Gaxera
metadata. The raw descriptor value retained in each region is diagnostic-only
and named `source_type` to keep the public context protocol-neutral.

The capture path emits a canonical serial dump of every copied memory-map
descriptor before allocator initialization. Entries are sorted by physical
base, type, and length for stable diagnostics, but they are not merged or
silently discarded. Each line records the original Limine type, Gaxera
classification, allocator eligibility, and reservation status.

The frame allocator has two stages:

1. A fixed-capacity bootstrap range allocator obtains unique, zeroed frames
   only from validated, page-aligned `MEMMAP_USABLE` ranges.
2. A segmented bitmap allocator takes ownership after its bitmap backing and
   all bootstrap allocations have been recorded.

`BootReservations` records physical ranges that must stay unavailable during
this handoff: bootstrap allocator output, allocator metadata, and page-table
frames. It is intentionally a boot-scoped transition type rather than a
general virtual-memory reservation service.

In Phase 4, only `MEMMAP_USABLE` ranges are allocator candidates. Bootloader
reclaimable, ACPI, executable/module, framebuffer, reserved, mapped-reserved,
and bad-memory ranges remain unavailable. Reclamation is deferred until each
range has an explicit ownership and lifetime policy.

## Consequences

All post-handoff memory consumers depend on `BootContext`, not Limine. The
kernel gains a bounded boot-information capacity; exceeding it is a
diagnosable boot failure rather than a truncation. A segmented bitmap avoids
metadata proportional to holes in the physical address space, at the cost of
fixed-capacity region bookkeeping and a linear region lookup.

The conservative allocation policy leaves reclaimable RAM unused temporarily,
but prevents accidental reuse of bootloader, ACPI, or firmware data while
their consumers are still being introduced.

## Alternatives Considered

Keeping Limine response structures for the kernel lifetime was rejected:
their mappings and ownership are bootloader-defined and must not survive a
Gaxera CR3 transition by accident.

A single dense bitmap from physical address zero to the highest address was
rejected because sparse maps make metadata scale with holes rather than usable
RAM. A frame-embedded free list was rejected because it complicates bootstrap
ownership and destroys the contents of free frames.
