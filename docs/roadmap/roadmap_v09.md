# Gaxera v0.9 Initiative: Hardware Enablement & Driver Infrastructure

This document outlines the engineering roadmap for **Initiative v0.9**, focusing on PCI Express bus enumeration, user-space driver framework, DMA capability memory infrastructure, VirtIO device drivers, and SMP multi-core scheduling.

---

## 1. Overview & Architectural Goals

The primary goal of Initiative v0.9 is to transform Gaxera from a capability microkernel foundation into a hardware-capable operating system capable of hosting unprivileged user-space device drivers on multi-core SMP hardware.

### Key Architectural Invariants:
1. **Microkernel Policy Separation:** PCI configuration space enumeration, driver selection, and device management are executed by unprivileged user-space servers (`pci_bus_server`). The kernel remains strictly mechanism-only.
2. **Capability Security for Hardware I/O:** Hardware I/O resources (MMIO BARs, IRQs, DMA physical memory) are accessed exclusively via kernel capabilities (`Mapping`, `Interrupt`, DMA capability object).
3. **Multi-Core SMP Scheduling:** The kernel manages AP core bringup via INIT-SIPI-SIPI, per-CPU runqueues, and cross-CPU IPI notifications, maintaining $O(1)$ scheduling complexity and quantum accounting.

---

## 2. Milestone Sequence

### Milestone 0.9.0: PCIe ECAM & User-Space PCI Bus Server [COMPLETED]
* **Primary Scope:** ACPI MCFG table parsing in kernel, ECAM `Mapping` capability generation, and user-space `pci_bus_server` bus scanning.
* **Deliverables:**
  * Parse ACPI MCFG table in `kernel/src/arch/x86_64/acpi.rs` to locate PCIe ECAM base physical address.
  * Create ECAM `Mapping` capability primitive granted to user-space `pci_bus_server`.
  * Implement `pci_bus_server` in user space to scan PCI buses (0..255), devices (0..31), and functions (0..7).
  * Parse vendor IDs, device IDs, BAR registers (IO vs MMIO), and IRQ pin assignments.
* **Acceptance Criterion:** QEMU integration test `test-pci-enum` demonstrates `pci_bus_server` enumerating PCI Express devices (including QEMU e1000/virtio-pci devices) and displaying BAR configurations.

### Milestone 0.9.1: Driver Framework & DMA Infrastructure [COMPLETED]
* **Primary Scope:** Capability delegation protocol (`pci_bus_server` $\rightarrow$ driver servers), generic driver lifecycle, and capability-backed DMA memory allocation.
* **Deliverables:**
  * Design and implement kernel capability primitive for cache-coherent, physically contiguous DMA allocations.
  * Implement capability delegation protocol allowing `pci_bus_server` to delegate BAR `Mapping` and IRQ `Interrupt` capabilities to specific driver tasks.
  * Build `libgaxera::driver` helper abstractions for type-safe driver construction.
* **Acceptance Criterion:** Host unit tests and QEMU integration test `test-driver-framework` verify capability-authorized DMA memory creation and BAR delegation.

### Milestone 0.9.2: VirtIO Foundation [COMPLETED]
* **Primary Scope:** VirtIO transport abstraction, Virtqueue descriptor chain state machine, available/used rings, and notification model.
* **Deliverables:**
  * Implement VirtIO legacy and PCI/MMIO transport wrappers in `libgaxera::virtio`.
  * Build allocation-free Virtqueue descriptor table, available ring, and used ring abstractions.
  * Implement shared-memory notification and IRQ signaling state machine.
* **Acceptance Criterion:** Host unit tests verify Virtqueue descriptor chain insertion, ring wrapping, and atomic buffer indexing.

### Milestone 0.9.3: VirtIO Block Storage Server [COMPLETED]
* **Primary Scope:** Unprivileged `virtio_block_server` driver, block request protocol, and disk sector read/write IPC interface.
* **Deliverables:**
  * Implement `virtio_block_server` executing in ring 3.
  * Handle asynchronous sector read and write requests over Virtqueues.
  * Expose block storage IPC endpoint for client applications.
* **Acceptance Criterion:** QEMU integration test `test-virtio-block` verifies guest disk read and write transactions against a QEMU drive image.

### Milestone 0.9.4: SMP Multi-Core Scheduling & Per-CPU Runqueues [COMPLETED]
* **Primary Scope:** ACPI MADT secondary AP discovery, AP INIT-SIPI-SIPI boot sequence, per-CPU `Scheduler` runqueues, and cross-CPU IPI notifications.
* **Deliverables:**
  * Discover secondary AP cores via ACPI MADT in `kernel`.
  * Boot AP cores using LAPIC INIT and Start-up IPI (SIPI) sequences.
  * Establish per-CPU `Scheduler` ready queues and CPU locality state.
  * Implement IPI vector handlers for cross-CPU rescheduling and thread migration.
* **Acceptance Criterion:** QEMU multi-core integration test `test-smp-schedule` demonstrates multi-core thread execution, IPI delivery, and zero task starvation across 4 CPU cores.

### Milestone 0.9.5: VirtIO Network Server & Zero-Copy Packet IPC
* **Primary Scope:** Unprivileged `virtio_net_server` driver, RX/TX Virtqueue management, MAC address initialization, and zero-copy packet IPC.
* **Deliverables:**
  * Implement `virtio_net_server` managing network packet reception and transmission.
  * Build zero-copy packet buffer IPC interface between network server and client applications.
* **Acceptance Criterion:** QEMU integration test `test-virtio-net` demonstrates loopback raw ethernet frame TX and RX processing.

---

## 3. Explicit Non-Goals for v0.9

* Hardware IOMMU (Intel VT-d / AMD-Vi) page table translation (reserved for v1.0).
* ACPI AML bytecode interpreter (reserved for v1.0).
* USB 3.0 xHCI driver stack.

---

### Milestone 0.9.1-recovery: Architectural Recovery Baseline [COMPLETED]
* **Primary Scope:** Authoritative 9-phase recovery program, ContiguousFrame DMA objects, bounded Mapping subregion derivation, 64-CPU SMP state isolation, W^X loader checks, and RankedLock 5-level lock hierarchy (ADR 0033).
* **Deliverable Tag:** `v0.9.1-recovery` (supersedes initial draft `v0.9.0`).

### Milestone 0.9.2: SMP Load Balancing & Inter-Core Scheduling
* **Primary Scope:** Inter-core work stealing, thread migration across 64 CPUs, cross-CPU TLB shootdown IPIs, and scheduler load metrics under ADR 0031.
* **Deliverables:**
  * Implement thread migration and work stealing queues across 64 CPUs (`MAX_CPUS = 64`).
  * Implement cross-CPU TLB shootdown IPI protocol in `kernel::arch::x86_64::paging`.
  * Add host and QEMU integration benchmarks verifying zero preemption deadlocks.

### Milestone 0.9.3: Persistent VFS & CoW Filesystem (`vfs_server`)
* **Primary Scope:** Multi-client Virtual File System (`vfs_server`) operating over `virtio_block_server` capabilities with Copy-on-Write semantics.
* **Deliverables:**
  * Build Ring-3 `vfs_server` managing directory hierarchies, file descriptors, and block cache.
  * Expose capability-mediated file open/read/write/stat IPC interface.

### Milestone 0.9.4: TCP/IP Network Stack (`net_stack_server`)
* **Primary Scope:** Ring-3 TCP/IP network protocol stack operating over `virtio_net_server` packet capabilities.
* **Deliverables:**
  * Build `net_stack_server` implementing Ethernet, ARP, IPv4, UDP, TCP, and DNS resolvers.
  * Provide socket IPC client abstractions in `libgaxera::net`.

### Milestone 0.9.5: Complete VirtIO Reference Platform
* **Primary Scope:** VirtIO-GPU 2D/3D display server and VirtIO-Input keyboard/mouse driver servers.
* **Deliverables:**
  * Build Ring-3 `virtio_gpu_server` for hardware-accelerated 2D/3D display rendering.
  * Build Ring-3 `virtio_input_server` for keyboard/mouse event routing to userspace.
  * Validate 100% complete VirtIO reference platform (Block, Net, GPU, Input) on QEMU.

---

## 3. Production Release & Hardware Expansion Strategy

### **v1.0.0 — Production Architectural Stability & Platform Release**
Gaxera `v1.0.0` represents **architectural completion and stability**, freezing the microkernel ABI, capability model, memory manager, IPC primitives, scheduler, driver framework, VFS, network stack, userspace runtime, and complete VirtIO reference platform.

### **Post-v1.0 Bare-Metal Hardware Expansion Series**
Following the `v1.0.0` architectural freeze, bare-metal hardware support grows incrementally without delaying core OS releases:
* **`v1.1.0` — Native Bare-Metal Storage:** Physical NVMe SSD (`nvme_server`) & AHCI SATA drivers.
* **`v1.2.0` — Native Bare-Metal Networking:** Physical Intel e1000/e1000e & Realtek NIC drivers.
* **`v1.3.0` — Native Bare-Metal USB & Input:** Physical xHCI USB 3.0 Host Controller & USB HID drivers.
* **`v1.4.0` — Native Bare-Metal Graphics:** Physical Intel i915 / VESA linear framebuffer drivers.

---

> [!NOTE]
> **Roadmap Status (2026-07-25):**  
> Baseline `v0.9.1-recovery` is verified and tagged. Implementation proceeds incrementally with Milestone 0.9.2 (SMP Load Balancing).
