# Driver & Hardware Interrupt Architecture (`driver_framework`)

> **Status:** Current  
> **Date:** 2026-07-23  
> **Initiative:** Initiative v0.9 (Milestones 0.9.0 & 0.9.1)  
> **Applies To:** `crates/pci_bus_server`, `crates/libgaxera`  

---

## 1. Executive Summary & Scope

The Gaxera Driver & Hardware Interrupt Architecture provides capability-authorized access to physical hardware resources (PCIe MMIO BARs, IOAPIC/LAPIC interrupts, and cache-coherent DMA physical memory) for unprivileged user-space driver processes.

### Key Architectural Invariants
1. **Microkernel Policy Separation:** Device discovery and driver assignment are executed in user space by `pci_bus_server`. The microkernel kernel remains strictly mechanism-only.
2. **Capability Authorization:** Hardware resources (`Mapping` capabilities for MMIO BARs, `Interrupt` capabilities for IRQ lines, `ContiguousFrameHandle` for DMA physical frames) are delegated via kernel capabilities.
3. **Hardware Isolation:** Device driver crashes do not crash the kernel or other isolated driver processes.

---

## 2. Component Design

### 2.1 PCIe ECAM Bus Server (`pci_bus_server`)
- Reads PCIe Enhanced Configuration Access Mechanism (ECAM) base address from ACPI MCFG table via kernel capability.
- Scans PCI bus hierarchy (Buses 0..255, Devices 0..31, Functions 0..7).
- Parses Vendor ID, Device ID, Subsystem ID, BAR registers (IO vs MMIO), and Interrupt Pin/Line assignments.

### 2.2 Capability Delegation Protocol
- `pci_bus_server` delegates BAR `Mapping` and IRQ `Interrupt` capabilities to specific user-space driver tasks (`virtio_block_server`, `virtio_net_server`).
- Receiver tasks acquire driver handles using `libgaxera::driver` abstractions.

### 2.3 Contiguous DMA Allocations (`ContiguousFrameHandle`)
- Provides physically contiguous, cache-coherent DMA memory allocations.
- Driver processes allocate DMA buffers via `ContiguousFrameHandle::from_parts(capability, phys_addr, size)`.

---

## 3. Verification

- QEMU integration test `test-pci-enum` verifies PCIe ECAM bus enumeration.
- QEMU integration test `test-driver-framework` verifies BAR delegation and capability DMA allocations.
