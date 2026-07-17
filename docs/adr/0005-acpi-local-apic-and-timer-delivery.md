# ADR 0005: ACPI, Local APIC, and Timer Delivery

## Status

Accepted

## Context

Phase 5 introduces firmware-table discovery, Local APIC MMIO, and the first
interrupt-enabled execution path. Phase 4 deliberately maps only usable RAM in
the HHDM and exposes no generic MMIO operation, so neither ACPI tables nor the
Local APIC may be accessed through an ambient physical-address conversion.

The phase needs only RSDP, XSDT, SDT headers, and MADT to locate the BSP's
Local APIC. It does not need AML evaluation, a general firmware service layer,
or a clocksource contract.

## Decision

Gaxera will parse only ACPI revision 2 or newer RSDP -> XSDT -> MADT data using
a small Gaxera-owned, bounds-checked byte parser. It validates signatures,
declared lengths, both RSDP checksums, every traversed SDT checksum, XSDT entry
alignment, and MADT subtable lengths. It honors exactly one MADT Local APIC
Address Override; malformed input is a diagnosable boot failure. It copies
table bytes and retains only owned `LocalApicInfo`, never raw ACPI pointers.

The `acpi` crate is not introduced in Phase 5. Its general physical-mapping,
I/O, timing, synchronization, and AML host interface exceeds the required
scope. A future phase may adopt it when AML or additional ACPI tables justify
the dependency and its host contract.

Paging will provide two narrow, reviewed operations: transient read-only,
write-back firmware-table mapping at one fixed virtual window, and one
permanent typed Local APIC mapping at a distinct fixed virtual address. The
APIC page must be outside usable RAM, the framebuffer, and the RAM-only HHDM;
it must have no competing Gaxera mapping. Its PTE uses NX plus the verified UC
cache selection. Firmware reads use the default PAT entry only after entry 0
is verified as WB; the APIC mapping uses PWT+PCD only after PAT entry 3 is
verified as UC. No general MMIO mapper is introduced.

Phase 5 supports only the bootstrap CPU in xAPIC MMIO mode. It validates CPU
APIC support, rejects active x2APIC mode, preserves the firmware LAPIC base
MSR address, and requires it to agree with the MADT-selected address. The IDT
installs timer and spurious gates before `sti`; both 8259 PICs are masked first.

The Local APIC timer is used only to prove periodic interrupt delivery. The
handler increments an atomic counter, performs EOI, and never allocates,
prints, locks, maps, or schedules. It has no calibrated frequency promise.
Calibration and clocksource selection are deferred to later scheduler/time work.

## Consequences

Phase 5 gives Gaxera its first safe interrupt-enabled idle state while keeping
firmware and MMIO access bounded and reviewable. The deterministic test masks
the timer after an exact target count and exits through `isa-debug-exit`; it
proves repeated delivery and EOI without relying on host timing.

ACPI reclaimable memory remains unavailable to the physical allocator. There
is no SMP, x2APIC, IOAPIC, MSI, AML, PIT/HPET/PM-timer calibration, generic
MMIO API, scheduler, or user-visible interrupt object in this phase.

## Alternatives Considered

Using the HHDM for ACPI or LAPIC access was rejected because the permanent
HHDM maps only usable RAM and must not create a write-back APIC alias. A
general ACPI crate was rejected as premature host-interface surface. A
QEMU-specific timer frequency was rejected because it would look like a clock
contract while proving only emulation behavior.
