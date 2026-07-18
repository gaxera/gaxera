# ADR 0011: User Address Space and Mapping Policy

**Status:** Accepted
**Date:** 2026-07-18
**Deciders:** Gaxera project

## Context

Phase 4 creates one Gaxera-owned kernel CR3 hierarchy with a higher-half
kernel mapping and a RAM-only HHDM. That hierarchy is correct for kernel
bootstrap but cannot be reused as a user address space: a user-accessible
mapping of the HHDM, page tables, stacks, heap, or kernel image would collapse
the isolation boundary.

M2A needs only one deterministic probe image. A generic user VM API,
arbitrary physical mappings, demand paging, and memory-object capabilities
would add authority and lifetime questions before they have users. The M2A
design must nevertheless establish the ownership and permission rules that
later `AddressSpace` and `MemoryObject` objects will preserve.

## Decision

Every M2A probe uses a new Gaxera-owned four-level CR3 root. The root is a
fresh physical frame allocated from the existing physical allocator. Its lower
half begins empty. Its upper half copies references to the active kernel root's
supervisor-only mappings; it does not copy any user-accessible mapping. Thus
kernel text, rodata, data, bootstrap/IST/transition stacks, heap, framebuffer,
page tables, APIC window, temporary firmware window, and HHDM remain present
for kernel execution but are inaccessible at CPL 3 because no leaf or
intermediate entry grants `USER_ACCESSIBLE`.

M2A fixes the initial 48-bit user layout:

| Range | Policy |
| --- | --- |
| `0x0000_0000_0000_0000`--`0x0000_0000_0000_0fff` | permanently unmapped null guard |
| `0x0000_0000_0040_0000` | one 4 KiB static probe code page, user R-X |
| `0x0000_0000_0080_0000`--`0x0000_0000_0080_0fff` | one 4 KiB user stack page, user R-W + NX |
| `0x0000_0000_0080_1000` | unmapped upper stack guard |
| all other lower-half addresses | unmapped in M2A |
| `0x0000_7fff_ffff_f000` and above | outside the M2A user allocation range; the noncanonical hole and high-half kernel mapping are never user mapped |

The user stack starts at `0x0000_0000_0080_1000`, immediately below its upper
guard page. The lower and upper guard policy makes stack over/underflow
deterministic and leaves room for later fixed-layout expansion without
committing to ASLR.

Only an architecture-private mapper may create M2A mappings. It accepts the
fixed code or stack virtual address and a frame that it just allocated; it
enforces 4 KiB alignment, canonical lower-half user range, the exact expected
permission set, W^X, and `USER_ACCESSIBLE`. It has no public arbitrary-frame,
arbitrary-virtual-address, or HHDM mapping operation. Every map/unmap flushes
the affected TLB entry; CR3 switches provide a complete non-global TLB flush
for an address-space transition.

User probe bytes are a fixed kernel-owned byte sequence copied into a fresh
frame before that frame is mapped R-X. There is no M2A ELF loader, linker,
relocation, code sharing, data page, or arbitrary user-write operation. The
probe toolchain therefore cannot introduce unsupported extended-state code.

Page-table frames, the root frame, code frame, and user-stack frame remain
kernel-owned physical allocations. The future `AddressSpace` and
`MemoryObject` objects will own the semantic mapping records and resource
charges; M2A intentionally has only one static proof instance.

## Consequences

M2A proves a genuine separate CR3 root and supervisor isolation without
inventing a general VM subsystem. The copied upper-half mapping keeps all
kernel exception and return code available while the U/S bit enforces the
security boundary. The fixed layout gives QEMU tests stable target addresses
for null, kernel/HHDM, and stack-guard denial proofs.

Address-space creation, mapping, and destruction are still kernel-internal
M2A mechanics. They are not capability operations and must not be exposed to
future user code until `AddressSpace`, `MemoryObject`, `ResourceDomain`, and
capability integration supply their lifetime and authority checks.

## Alternatives Considered

**Run the probe under the kernel CR3:** rejected because supervisor mappings
would leak into the first user-mode boundary and invalidate the isolation
claim.

**Create a completely kernel-free user root:** rejected for M2A because a
ring-3 exception needs executable kernel IDT/handler/return paths. A shared
supervisor-only higher half is the conventional and bounded intermediate
design.

**Expose a generic page-table mapper:** rejected because it would let early
callers bypass future `MemoryObject` capability and resource-accounting rules.

**Use huge pages or user ASLR now:** rejected because neither improves the
basic isolation proof and both enlarge mapping, TLB, and test complexity.
