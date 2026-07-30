# Gaxera v0.9 Initiative: Hardware Enablement & Storage Platform

> **Status:** Formally Closed & Completed (100%)  
> **Completion Date:** 2026-07-27  
> **Final Tag:** `pre-v1.0-phase-5`  
> **Scope:** Milestones 0.9.0 through 0.9.5 + Pre-v1.0 Audit & Hardening Series (Phases 1–5).

This document outlines the authoritative engineering roadmap for **Initiative v0.9**, focusing on PCI Express bus enumeration, user-space driver framework, DMA capability memory infrastructure, SMP multi-core scheduling, and the GaxFS Native Storage Platform.

---

## 1. Overview & Architectural Goals

The primary goal of Initiative v0.9 is to transform Gaxera from a capability microkernel foundation into a hardware-capable operating system with a native, capability-secured storage platform.

### Key Architectural Invariants:
1. **Microkernel Policy Separation:** PCI configuration space enumeration, driver selection, and storage services are executed by unprivileged user-space servers (`pci_bus_server`, `vfs_server`, `gaxfs_server`). The kernel remains strictly mechanism-only.
2. **Capability Security for Hardware & Filesystems:** Hardware I/O resources and filesystem objects are accessed exclusively via kernel capabilities (`Mapping`, `Interrupt`, `CapabilityHandle`).
3. **Multi-Core SMP Scheduling:** The kernel manages AP core bringup via INIT-SIPI-SIPI, per-CPU runqueues, and cross-CPU IPI notifications, maintaining $O(1)$ scheduling complexity and quantum accounting.

---

## 2. Authoritative Milestone Sequence

### Milestone 0.9.0: PCIe ECAM & User-Space PCI Bus Server [COMPLETED]
* **Primary Scope:** ACPI MCFG table parsing in kernel, ECAM `Mapping` capability generation, and user-space `pci_bus_server` bus scanning.
* **Deliverables:**
  * Parse ACPI MCFG table in `kernel/src/arch/x86_64/acpi.rs` to locate PCIe ECAM base physical address.
  * Create ECAM `Mapping` capability primitive granted to user-space `pci_bus_server`.
  * Implement `pci_bus_server` in user space to scan PCI buses (0..255), devices (0..31), and functions (0..7).
* **Acceptance Criterion:** QEMU integration test `test-pci-enum` demonstrates `pci_bus_server` enumerating PCI Express devices and displaying BAR configurations.

### Milestone 0.9.1-recovery: Architectural Recovery Baseline [COMPLETED]
* **Primary Scope:** Authoritative 9-phase recovery program, ContiguousFrame DMA objects, bounded Mapping subregion derivation, 64-CPU SMP state isolation, W^X loader checks, and RankedLock 5-level lock hierarchy (ADR 0033).
* **Deliverable Tag:** `v0.9.1-recovery` (supersedes initial driver framework draft).

### Milestone 0.9.2: SMP Load Balancing & Inter-Core Scheduling [COMPLETED]
* **Primary Scope:** Inter-core work stealing, thread migration across 64 CPUs, cross-CPU TLB shootdown IPIs, and CpuAffinityMask under ADR 0031.
* **Deliverable Tag:** `v0.9.2` (`ea190f2`). 69 kernel-core & 22 kernel unit tests passed with 0 clippy warnings.

### Milestone 0.9.3: Persistent VFS & GaxFS Native Storage Platform [COMPLETED]
* **Primary Scope:** GaxFS First-Principles Architecture (ADR 0034), capability-mediated object storage, generic Copy-on-Write storage engine (`gax_storage_engine`), three-layer query architecture (`query_planner` & GaxQL), event stream (`gaxfs_event_log`), vector search (`gaxfs_vector_index`), and foundation types (`gaxfs_types`).
* **Deliverables & Progress:**
  * **Phase 1 (Design & Specification Freeze) [COMPLETED]:** 12 authoritative specifications in `docs/architecture/` and ADR 0034.
  * **Phase 2 (Foundation & Trait Implementation) [COMPLETED]:** `gaxfs_types` (RFC 9562 UUIDv7, `CapabilityHandle`, abstract provider traits).
  * **Phase 3 (Core Storage Engine & Journal) [COMPLETED]:** `gax_storage_engine` (dual-superblock CoW commits, erase-block extent allocation, graph links, bitrot scrubbing).
  * **Phase 4 (Event Log & Vector Indexing) [COMPLETED]:** `gaxfs_event_log` (zero-copy ring buffer), `gaxfs_vector_index` (FWHT, 4-bit Lloyd-Max, RaBitQ scale renormalization, SIMD capability isolation).
  * **Phase 5 (Three-Layer Query Engine) [COMPLETED]:** `query_planner` (GaxQL AST, cost optimization, index routing, candidate set intersection, fluent builder).
  * **Phase 6 (System Integration & Stress Testing) [COMPLETED]:** 25 specialized v0.9.3 tests, 88 workspace passing tests, end-to-end integration suite.

### Milestone 0.9.4: VirtIO Network Server & GaxNet Native Network Platform [COMPLETED]
* **Primary Scope:** GaxNet First-Principles Architecture (ADR 0035), unprivileged `virtio_net_server` driver, `net_stack_server` protocol engine, `resolver_server`, `crypto_server`, `libgaxera::net`, 4-stage verification, mandatory fuzz testing, performance benchmarks, and continuous evidence collection.
* **Deliverables & Progress:**
  * **Phase 1 (Architectural Specification & ADR 0035 Freeze) [COMPLETED]:** 16 canonical specifications in `docs/architecture/` and master ADR 0035.
  * **Phase 2A (Core Network Types & Descriptors) [COMPLETED]:** `crates/net_types` (addresses, headers, `NetRights`, policy constraints, object handles, `TransportInstance`, `FrameDescriptor`).
  * **Phase 2B (Shared Memory, Provider ABI & Versioning) [COMPLETED]:** `crates/net_types` (`PacketRingHeader`, SPSC wraparound math, backpressure policies, provider traits, common provider lifecycle, version negotiation headers).
  * **[PUBLIC ABI FREEZE CHECKPOINT] [COMPLETED]:** Immutable freeze of public memory layouts, provider trait signatures, capability structures, and IPC formats.
  * **Phase 3 (VirtIO-Net Hardware Driver Server) [COMPLETED]:** `virtio_net_server` (PCI BAR mapping, Virtqueues, DMA `ContiguousFrame`, MSI-X IRQ handling, SPSC `PacketRing` handoff).
  * **Phase 4A (Core Link/Network Stack) [COMPLETED]:** `net_stack_server` (Ethernet II, ARP cache, IPv4/IPv6 routing, ICMP ping, UDP datagram routing).
  * **Phase 4B (Stateful TCP & UDP Engines) [COMPLETED]:** `net_stack_server` (RFC 793 TCP state machine, 3-way handshake, SYN/ACK, sequence tracking, FIN connection teardown, NewReno 3-dupACK Fast Retransmit/Recovery, dynamic RTO retransmission timer queue with exponential backoff, UDP socket table array and port demuxing).
  * **Phase 5 (Decoupled Services & Client APIs) [COMPLETED]:** `resolver_server` (RFC 1035 wire query encoder & IPv4 answer parser), `crypto_server` (ChaCha20-Poly1305 AEAD payload encryption & 128-bit MAC tag authentication), `libgaxera::net` (Native API & BSD Socket Virtualization Table).
  * **Phase 6 (System Verification & Unit Test Suite) [COMPLETED]:** Unit & integration tests, full workspace tests, 0 compiler warnings, 0 clippy warnings, 100% `cargo fmt` compliant.

### Milestone 0.9.5: Complete VirtIO Reference Platform [COMPLETED]
* **Primary Scope:** VirtIO-GPU 2D/3D display server and VirtIO-Input keyboard/mouse driver servers.
* **Deliverables & Progress:**
  * **Phase 1 (Architecture & Specification Freeze) [COMPLETED]:** [ADR 0036](../adr/0036-virtio-gpu-input-reference-platform.md) and master specification `docs/architecture/virtio_reference_platform.md`.
  * **Phase 2 (VirtIO-GPU Display Server) [COMPLETED]:** Ring-3 `virtio_gpu_server` (PCI Express BAR mapping, Virtqueues, 2D resource creation, memory backing, scanout flush, RGBA test pattern framebuffer).
  * **Phase 3 (VirtIO-Input Driver Server) [COMPLETED]:** Ring-3 `virtio_input_server` (PCI Express BAR mapping, Virtqueues, EV_KEY/EV_REL event decoding, FocusHandle capability scoping, zero keylogging).
  * **Phase 4 (System Integration & Visual Proof) [COMPLETED]:** Full VirtIO reference platform (Block, Net, GPU, Input) verified on QEMU, visual PNG screenshot dump captured (`display_scanout.png`), full un-truncated logs archived (`checkpoint-20-v0.9.5-virtio-platform`).

### Pre-v1.0 Audit & Hardening Series [COMPLETED]
* **Primary Scope:** 5-phase comprehensive system audit, protocol hardening, microkernel verification, and evidence logging in preparation for formal `v1.0.0` release.
* **Deliverables & Progress:**
  * **Phase 1 (GaxNet & Ring-3 Server Hardening) [COMPLETED]:** Hardened `net_stack_server` (RFC 793 TCP state machine, FIN teardown, NewReno 3-dupACK Fast Retransmit/Recovery, RFC 6298 RTO timers, UDP socket tables), `crypto_server` (ChaCha20-Poly1305 AEAD payload encryption & MAC tag verification), `resolver_server` (RFC 1035 wire DNS query encoder & IPv4 answer parser), initialized active `_start` IPC event loops across all 8 Ring-3 server binaries, and verified 100% test coverage with raw execution evidence logs across checkpoints 15–20.
  * **Phase 2 (Memory Foundation & Slab Allocation Audit) [COMPLETED]:** Hardened physical frame bitmap allocator (double-free rejection & ACPI exclusion), recursive PML4 unmapping (`W^X` page permission enforcement), O(1) typed `SlabCache<T>` (freelist traversal & interior pointer rejection), Buddy DMA contiguous frame allocator ($2^N$ 4KiB page order alignment), 48-bit canonical address bounds, and SMAP-safe `copy_from_user` `EFAULT` recovery. Archival log recorded in `checkpoint-21-pre-v1.0-phase2-memory`.
  * **Phase 3 (Capability Engine & Revocation Matrix Audit) [COMPLETED]:** Verified rights subset attenuation matrix (rights escalation rejection), 3-tier multi-CSpace cascade revocation, generational handle invalidation (`StaleHandle` protection), object destruction handle clearing, and zero-allocation transactional capability transfers with atomic rollback. Archival log recorded in `checkpoint-22-pre-v1.0-phase3-capability`.
  * **Phase 4 (Multi-Client IPC & Multi-Core Scheduler Audit) [COMPLETED]:** Audited `WaitSet` event multiplexing and notification handling, 64-core scheduler domain topology work stealing across `CpuAffinityMask` boundaries, and lower-to-higher CPU ID lock ordering hierarchies during thread migration across scheduling domains.
  * **Phase 5 (Full System Integration & v1.0 Release Candidate Audit) [COMPLETED]:** Enforced syscall `Rights::WRITE` / `Rights::READ` endpoint privileges in `kernel/src/arch/x86_64/syscall.rs`, wired TCP retransmit packet frames directly onto shared memory `PacketRing` transmit slots in `net_stack_server`, added live UDP DNS wire resolution for public domains (`google.com`, `github.com`), implemented `WaitSet` event signal coalescing and capacity limit unit tests, added 64-core scheduler full-mesh migration and multi-queue work-stealing stress tests, verified 100% green workspace tests (99/99), zero clippy warnings, `cargo fmt` formatting, and UEFI QEMU boot execution. Archival log recorded in `checkpoint-23-pre-v1.0-phase5-release`.

---

## 3. Explicit Non-Goals for v0.9

* Hardware IOMMU (Intel VT-d / AMD-Vi) page table translation (reserved for v1.0).
* ACPI AML bytecode interpreter (reserved for v1.0).
* USB 3.0 xHCI driver stack (reserved for v1.3).

---

## 4. Production Release & Hardware Expansion Strategy

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
> **Roadmap & Audit Audit Status (2026-07-30):**  
> Pre-v1.0 Audit & Hardening Series (Phases 1-5) is 100% COMPLETED and verified at tag `pre-v1.0-phase-5`. All 99 workspace tests pass green. The microkernel ABI, memory manager, capability matrix, and IPC invariants are locked for the **v1.0.0 Architectural Baseline**. Subsystem maturity levels are explicitly classified in `README.md` and release evidence: VirtIO QEMU Reference Platform (BSP execution); GaxFS CoW Storage Engine Prototype; GaxNet Ring-3 Protocol Stack & AEAD Encryption Prototype; 64-Core SMP Scheduler Topology Model. Platform is ready for formal `v1.0.0` baseline tagging!
