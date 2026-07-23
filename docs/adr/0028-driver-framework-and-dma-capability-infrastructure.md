# ADR 0028: Driver Framework & DMA Capability Infrastructure Architecture

> **Status:** Proposed  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.9.1 — Driver Framework & DMA Infrastructure (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `kernel`, `kernel-core`, `libgaxera`, `pci_bus_server`  

---

## Context & Problem Statement

Device drivers in Gaxera run as unprivileged ring-3 user processes (`virtio_block_server`, `virtio_net_server`). Drivers require capability authorization to perform four hardware actions:

1. Accessing hardware MMIO registers mapped via PCI BARs.
2. Reading and writing the device's own PCI configuration registers (e.g., Bus Mastering, Memory Space Enable, MSI/MSI-X).
3. Receiving hardware interrupt notifications.
4. Allocating cache-coherent, physically contiguous DMA memory buffers for hardware engines.

---

## Hardware Authority Security Invariants

> **Invariant 1 — Capability Provenance:**  
> A driver never gains hardware authority by constructing physical addresses or independently discovering hardware. All hardware authority originates exclusively through capability delegation by the kernel and `pci_bus_server`.
> 
> **Invariant 2 — Kernel-Enforced Derivation Security:**  
> User-space tasks (including `pci_bus_server`) cannot manufacture or forge capabilities. Creating a derived sub-region capability requires executing kernel system call `sys_derive`. The kernel validates that the derived physical memory window is a **strict subset** of the parent physical range before allocating the child capability in CSpace.

---

## Technical Decisions

### 1. `ObjectType::ContiguousFrame` Kernel Object & Capability Invocation
- `kernel-core` defines symbolic `ObjectType::ContiguousFrame`.
- A `ContiguousFrame` capability object represents a physically contiguous range of $2^N$ page frames allocated from the kernel physical frame allocator.
- **Object-Oriented Invocation:** Physical address resolution is **not** a top-level system call. Physical address translation is performed via standard capability invocation on the object itself (ADR 0015):
  ```rust
  sys_invoke(contiguous_frame_handle, ContiguousFrameOp::QueryPhysicalAddress)
  ```
- The kernel validates that `contiguous_frame_handle` is a valid `ContiguousFrame` capability with `Rights::READ` and returns the physical base address.

### 2. Kernel-Enforced Device-Scoped ECAM Mapping Derivation
- Access to a device's PCI configuration registers (e.g. enabling Bus Master bit 2 in the Command register) is authorized via a **Device-Scoped ECAM `Mapping` capability**.
- `pci_bus_server` holds the segment-level ECAM `Mapping` capability.
- To assign configuration authority for a specific device, `pci_bus_server` executes `sys_derive`:
  ```rust
  sys_derive(segment_ecam_handle, device_ecam_offset, 4096, derived_rights)
  ```
- The kernel validates that the 4KB window `[device_ecam_offset, device_ecam_offset + 4096)` is a strict subset of the segment `Mapping` physical range, allocates the derived `Mapping` capability slot in CSpace, and links it in the derivation tree.
- The derived 4KB `Mapping` capability is delegated to the driver process via IPC, authorizing the driver to map and manipulate its own configuration space registers without accessing unrelated PCI devices.

### 3. Capability Delegation Protocol & Driver Matching
- `pci_bus_server` performs **driver matching, device assignment, and resource delegation**.
- When a driver process launches, it requests device assignment from `pci_bus_server` via IPC.
- Upon matching a compatible device, `pci_bus_server` delegates:
  1. **Device-Scoped ECAM `Mapping` Capability:** Derived 4KB config page capability for Command/Status/MSI register management.
  2. **BAR `Mapping` Capabilities:** Physical MMIO BAR page capabilities for device registers.
  3. **IRQ `Interrupt` Capability:** Bound to the device's hardware interrupt vector.

### 4. Symbolic ABI Specification
- ABI definitions use symbolic names (`ObjectType::ContiguousFrame`, `ContiguousFrameOp::QueryPhysicalAddress`, `PciConfigOp`) rather than fixed numeric values, allowing integer discriminants to be assigned deterministically in `gaxera-abi`.

### 5. Type-Safe Runtime Abstractions (`libgaxera::driver`)
- `libgaxera` exports type-safe wrappers:
  - `ContiguousFrameHandle`: Safe owned wrapper for contiguous DMA physical frames.
  - `PciDeviceConfig`: Safe wrapper for reading/writing device-scoped ECAM configuration registers.
  - `DriverEntry`: Trait for driver lifecycle management.

---

## Non-Goals for ADR 0028

- Untrusted driver loading or dynamic binary downloading (drivers are trusted ELF binaries supplied by the system image).
- Hardware IOMMU page table translation (reserved for v1.0).

---

## Consequences & Invariants

1. **Explicit Security Boundary:** Kernel `sys_derive` enforces strict physical address range containment for all derived `Mapping` capabilities.
2. **Object-Oriented Consistency:** Physical address translation uses standard `sys_invoke` capability operation semantics.
3. **Device-Scoped Configuration Protection:** Drivers receive a 4KB derived ECAM `Mapping` capability restricted exclusively to their assigned device.
