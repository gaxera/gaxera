# ADR 0027: PCIe ECAM Segment Discovery & User-Space PCI Bus Server Architecture

> **Status:** Proposed  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.9.0 — PCIe ECAM & User-Space PCI Bus Server (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `kernel`, `libgaxera`, `init`, `pci_bus_server`  

---

## Context & Problem Statement

Device discovery and bus management are fundamental to hardware enablement. In traditional monolithic operating systems, PCI Express bus scanning, BAR allocation, and MSI configuration are executed inside kernel space, embedding hardware-specific policy into the kernel.

In Gaxera's capability microkernel architecture:

1. **Kernel Mechanism vs. User-Space Policy:** The kernel remains strictly mechanism-only. Parsing PCI IDs, discovering capabilities, managing device databases, and matching drivers belong in user space.
2. **Capability Authorization:** Access to PCI configuration space MMIO registers must be authorized by kernel capability primitives (`Mapping` objects).
3. **Passive Discovery Boundary:** Initial PCI bus enumeration must be purely passive (read-only configuration space inspection), avoiding firmware state mutation or BAR address corruption during discovery.

---

## Technical Decisions

### 1. Generic `Mapping` Capability Physical Range Invariant
- A `Mapping` capability is a generic, platform-neutral kernel object. It encapsulates a specific physical memory address range `[phys_start, phys_start + length)` and authorizes virtual address space mapping strictly within that range.
- `Mapping` capabilities do **not** contain hardware-specific or protocol-specific policy inside `kernel-core`.
- Security isolation is enforced by physical address range containment:
  - During PCI discovery, the kernel creates `Mapping` capabilities whose physical address ranges correspond **exclusively** to the ECAM segments described by ACPI MCFG.
  - Holding an ECAM `Mapping` capability authorizes access only to those physical configuration-space frames.
  - Device MMIO BAR regions reside in separate physical address ranges and require separate `Mapping` capabilities created and delegated later during driver assignment (Milestone 0.9.1).

### 2. ACPI MCFG Segment Discovery (Multiple PCI Domains)
- The kernel parses all entries in the ACPI **MCFG** (Memory-Mapped Configuration Space) table during boot.
- The kernel does not assume a single `0..255` bus segment. It extracts each segment group:
  - `pci_segment_group_number`: Domain/segment ID (e.g. `0`).
  - `base_address`: Physical MMIO base address.
  - `start_bus_number`: First bus in segment (e.g. `0`).
  - `end_bus_number`: Last bus in segment (e.g. `255`).
- The kernel creates one `Mapping` capability per validated ECAM segment physical range.

### 3. Passive Read-Only Bus Enumeration (`pci_bus_server`)
- `pci_bus_server` receives segment `Mapping` capabilities from `init` and maps them into its virtual address space.
- For each segment, `pci_bus_server` iterates through `start_bus..=end_bus`, Device (`0..31`), and Function (`0..7`):
  ```text
  ECAM_Offset = (((Bus - StartBus) << 20) | (Device << 15) | (Function << 12)) + Register
  ```
- Scanning is **strictly passive and read-only**:
  - Reads Vendor ID & Device ID (checking `vendor_id != 0xFFFF`).
  - Reads Class Code, Subclass, Programming Interface, and Revision ID.
  - Reads Header Type (identifying single-function vs multi-function devices).
  - Identifies raw BAR register values (`0x10`..`0x24`) without writing `0xFFFFFFFF` size probes.
  - Reads Interrupt Line (`0x3C`) and Interrupt Pin (`0x3D`).

### 4. PCI Capability Linked-List Parsing
- If bit 4 of the Status register (`Capabilities List` flag) is set, `pci_bus_server` parses the linked-list of PCI Capabilities starting at `0x34` (`Capability Pointer`).
- The server traverses capability nodes (`cap_id`, `next_ptr_offset`), recording offsets for:
  - `0x01`: Power Management
  - `0x05`: MSI (Message Signaled Interrupts)
  - `0x09`: Vendor-Specific Capability (e.g. VirtIO MMIO/PCI structures)
  - `0x10`: PCI Express Capability
  - `0x11`: MSI-X
- Capability locations are recorded in the device database; actual capability configuration is deferred.

### 5. Resource Ownership & Authority Delegation Model
- **Kernel Ownership:** The kernel owns all physical hardware resources (memory frames, IRQ vectors, address spaces) and enforces capability range containment.
- **`pci_bus_server` Ownership:** `pci_bus_server` owns the logical PCI device database, capability offset map, and driver matching policy in user space.
- **Driver Isolation:** Driver processes (`virtio_block_server`, `virtio_net_server`) never enumerate PCI directly.
- **Delegation Protocol:** Driver processes request compatible devices from `pci_bus_server`. BAR `Mapping` capabilities and IRQ `Interrupt` capabilities are delegated by `pci_bus_server` to driver tasks only after device assignment is authorized in Milestone 0.9.1.

---

## Explicit Non-Goals for ADR 0027

The following responsibilities are **out of scope** for Milestone 0.9.0 and are deferred to Milestone 0.9.1 (Driver Framework):
- Sizing BAR registers via `0xFFFFFFFF` write probes.
- Allocating physical MMIO addresses for unassigned BARs.
- Enabling Bus Mastering (bit 2 of Command register).
- Configuring MSI / MSI-X interrupt vectors.
- Device initialization or reset state transitions.
- Driver selection and capability delegation to driver tasks.

---

## Consequences & Invariants

1. **Orthogonal Capability Model:** `Mapping` objects remain strictly platform-neutral physical address range capabilities.
2. **Passive Safety:** Bus enumeration is 100% read-only, preventing firmware BAR mapping corruption.
3. **Multi-Domain Support:** Enumerate arbitrary PCI segment groups from ACPI MCFG.
4. **Clean Ownership Boundaries:** Physical hardware owned by kernel; logical device database owned by `pci_bus_server`; drivers receive only delegated capabilities.
