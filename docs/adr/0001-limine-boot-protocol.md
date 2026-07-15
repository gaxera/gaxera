# ADR 0001: Limine Boot Protocol Contract

## Status

Accepted

## Context

Gaxera is a bare-metal capability-based Rust microkernel starting from scratch. To boot our kernel, we must establish a hardware and software handoff interface with a bootloader. We require a protocol that:

1. Directly handles loading 64-bit ELF binaries.
2. Initializes the CPU into a standard state (64-bit Long Mode, paging enabled) to avoid writing extensive 16-bit real-mode bootstrap assembly.
3. Provides structured hardware information (memory maps, UEFI framebuffer parameters).

We have selected the **Limine Boot Protocol (v12.4.2)** as our bootloader and protocol boundary.

## Decision

We standardize on the Limine Boot Protocol version `v12.4.2`. The boot contract establishes the following entry state and layout conventions:

### 1. CPU State at Entry (`_start`)

* **Mode:** 64-bit Long Mode with paging active.
* **Interrupts:** Disabled (`cli`).
* **CR3:** Points to a temporary 4-level page table mapping the kernel and the bootloader structures.
* **GDT:** Set up with valid flat 64-bit code and data segments.
* **Stack:** `RSP` points to a valid bootloader-allocated stack.
* **Flags:** Direction flag cleared (`cld`).

### 2. Memory Layout (Higher Half Direct Map)

* **Kernel VMA:** The kernel ELF is mapped at virtual address base `0xffffffff80000000` (Higher Half VMA). This base is selected to match the x86-64 `kernel` code model convention (enabling optimal 32-bit sign-extended relative symbol offsets), rather than as a strict requirement of the Limine protocol itself.
* **Direct Mapping (HHDM):** Limine maps the physical memory pages that correspond to specific memory map entries and the framebuffer starting at a direct virtual mapping offset (retrieved via the HHDM request). This offset allows virtual-to-physical translation within those regions, rather than providing an ambient license to dereference arbitrary physical addresses outside the designated mapped areas.

### 3. Handoff Requests & Responses

* **Protocol Request Structs:** The kernel declares its request parameters (Framebuffer, Memory Map, HHDM, and RSDP) as used static records in a dedicated writable `.requests` section, allowing Limine to populate response pointers during handoff.
* **Bootloader Resolution:** Limine parses the ELF section, matches the request signatures, populates the response pointers inside the structures, and launches our kernel entry point (`_start`).

### 4. Linker Requirements

* **Executable Format:** We compile the kernel as a statically linked ELF64.
* **Section Alignment:** A custom linker script must map sections (`.text`, `.rodata`, `.data`, `.bss`) aligned to 4KiB boundaries and ensure the `.limine_requests` section resides in a loadable segment.

## Consequences

* We avoid real-mode initialization code, bootstrap memory setup, and raw A20 line toggling.
* Our kernel binary must remain a valid ELF64 with loadable segments corresponding to the custom linker script.
* Any memory modifications must preserve page mappings containing bootloader structures until a custom virtual memory manager is loaded.
* Paging tables and physical frame information are retrieved directly from the Limine memory map response.
