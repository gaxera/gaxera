# What Our Kernel Must Be — The Complete Requirements Extraction

> **Status:** Canonical | **Version:** 1.0 | **Last Updated:** 2026-07-12
> **Related:** [Roadmap](../roadmap/roadmap_v01.md), [Constitution](../governance/constitution.md)

**Purpose:** Every hard requirement, constraint, and property our kernel must satisfy, explicitly extracted from the Technical Specification. This serves as an evaluation checklist rather than an independent architectural source.

---

## 1. Non-Negotiable Architectural Properties

These come directly from the Constitution and Architectural Invariants. Violating any of these disqualifies a candidate.

| ID | Requirement | Source |
| --- | --- | --- |
| K-01 | **Microkernel.** Minimal ring-0 code. Everything except threads, address spaces, IPC, capabilities, interrupts, and page-level memory lives in user space. Drivers, filesystems, networking, audio, display — all user-space. | [Technical Specification](technical_spec.md) §2.1, Constitution §5 |
| K-02 | **Capability-based access control.** No ambient authority. No root. No sudo. Every resource access is mediated by an unforgeable capability token. This is the *only* access control model. | [Technical Specification](technical_spec.md) §2.5, Constitution §5, SEC-01 |
| K-03 | **Fault isolation.** A crashed user-space service (driver, filesystem, compositor) must NEVER bring down the kernel. The kernel continues running. The supervisor restarts the crashed service. | [Technical Specification](technical_spec.md) INV-01 |
| K-04 | **Rust + minimal assembly.** The kernel is written in Rust. Assembly is permitted only for: context switching, boot entry, CPU-specific register manipulation. No C. No C++. | [Technical Specification](technical_spec.md) KRN-01 |
| K-05 | **UEFI-only boot.** No BIOS. No legacy 16-bit real mode. The kernel receives control from a UEFI bootloader (Limine). | [Technical Specification](technical_spec.md) §2.1, Graveyard GRV-007 |
| K-06 | **x86_64 first.** Primary target is `x86_64-unknown-none`. ARM64 is a future consideration, not a v0.1 or v1.0 requirement. | [Technical Specification](technical_spec.md) KRN-01 |
| K-07 | **System state and user data architecturally separable.** The kernel's design must not conflate system state with user data at the memory/address-space level. | [Technical Specification](technical_spec.md) INV-02 |
| K-08 | **Kernel transports mechanism, not semantics.** IPC carries raw bytes + capability references. The kernel does NOT interpret message content. Semantic typing is user-space (IDL-generated). | [Technical Specification](technical_spec.md) INV-03, IPC-01 |

---

## 2. Kernel Object Model

The kernel exposes exactly **10 kernel objects**. Not more, not fewer (at v1). Every interaction with the system goes through these objects and their capabilities.

| Object | What It Does |
| --- | --- |
| **Thread** | Unit of CPU execution. Bound to an AddressSpace. |
| **AddressSpace** | Virtual memory container. Page tables, mappings. Isolates processes from each other. |
| **CapabilitySpace** | Per-thread/per-process table of capability tokens. The "permission wallet." |
| **Endpoint** | IPC rendezvous point. Synchronous call/reply happens here. |
| **Notification** | Lightweight async signaling (bitmask). Used for interrupts, wakeups, events. |
| **MemoryObject** | A region of physical memory that can be mapped into an AddressSpace. |
| **Mapping** | The binding between a MemoryObject and a virtual address range in an AddressSpace. |
| **InterruptObject** | Delivers hardware interrupts to a user-space driver thread via Notification. |
| **TimerObject** | Kernel-managed timer that fires a Notification at a specified time or interval. |
| **SchedulingContext** | Controls CPU time allocation for a thread or group of threads. |

**Source:** [Technical Specification](technical_spec.md) KRN-02.

---

## 3. IPC Requirements

IPC is the single most critical performance path in a microkernel. Everything — drivers, filesystems, the compositor, the AI pipeline — communicates through it.

| ID | Requirement | Source |
| --- | --- | --- |
| IPC-01 | Synchronous call/reply (fast path for short messages). | [Technical Specification](technical_spec.md) §2.4 |
| IPC-02 | Asynchronous notifications (lightweight bitmask signaling). | [Technical Specification](technical_spec.md) §2.4 |
| IPC-03 | Shared memory regions for bulk data transfer (zero-copy). | [Technical Specification](technical_spec.md) §2.4 |
| IPC-04 | Capability passing through IPC (send a capability token as part of a message). | [Technical Specification](technical_spec.md) §2.5 |
| IPC-05 | **No broadcast in kernel.** Broadcasting/pub-sub is a user-space event broker concern. | |
| IPC-06 | Fast-path message size determined by benchmarking, not hardcoded. | |
| IPC-07 | **Target: < 1μs same-core round trip** on reference hardware. | [Technical Specification](technical_spec.md) PERF-03 |

---

## 4. Scheduler Requirements

The scheduler must support multiple workload classes simultaneously, because the OS runs real-time audio, a GPU compositor, background AI inference, and user applications concurrently.

| ID | Requirement | Source |
| --- | --- | --- |
| SCHED-01 | **Deadline scheduling class** (for audio, compositor). | [Technical Specification](technical_spec.md) KRN-04 |
| SCHED-02 | **Fixed-priority real-time class.** | [Technical Specification](technical_spec.md) KRN-04 |
| SCHED-03 | **Interactive fair class (EEVDF).** | [Technical Specification](technical_spec.md) KRN-04 |
| SCHED-04 | **Throughput fair class** (for batch/background work). | [Technical Specification](technical_spec.md) KRN-04 |
| SCHED-05 | **Maintenance/Idle class** (for housekeeping, dedup, indexing). | [Technical Specification](technical_spec.md) KRN-04 |
| SCHED-06 | Priority inheritance (to prevent priority inversion in IPC chains). | |
| SCHED-07 | CPU affinity support. | |
| SCHED-08 | User-adjustable priority (through capability-gated API). | |
| SCHED-09 | Foreground focus = scheduling hint, not blanket priority boost. | [Technical Specification](technical_spec.md) KRN-05 |
| SCHED-10 | SMP support (multi-core). The user's machine has 8 cores / 16 threads. | |

---

## 5. Memory Management Requirements

| ID | Requirement | Source |
| --- | --- | --- |
| MEM-01 | Virtual memory with 4-level (or 5-level) x86_64 page tables. | [Technical Specification](technical_spec.md) §2.3 |
| MEM-02 | Per-process address space isolation. | [Technical Specification](technical_spec.md) §2.3 |
| MEM-03 | ASLR and KASLR enabled by default. | [Technical Specification](technical_spec.md) MEM-01 |
| MEM-04 | Hierarchical memory pressure management (8-step OOM sequence). | [Technical Specification](technical_spec.md) MEM-02 |
| MEM-05 | Dynamic memory compression (adapts to actual conditions). | |
| MEM-06 | Capability-gated shared memory between processes. | |
| MEM-07 | Memory-mapped file support. | |
| MEM-08 | Stack sizing control. Per-process heap. | |
| MEM-09 | **Target: < 1 GB base OS idle RAM.** | [Technical Specification](technical_spec.md) MEM-03 |

---

## 6. Security & Isolation Requirements

| ID | Requirement | Source |
| --- | --- | --- |
| SEC-01 | **No root.** No single account or process with unlimited authority. | [Technical Specification](technical_spec.md) SEC-01, Constitution §5 |
| SEC-02 | Capability revocation (remove a granted capability). | [Technical Specification](technical_spec.md) SEC-02 |
| SEC-03 | Time-bounded capability leases. | [Technical Specification](technical_spec.md) SEC-02 |
| SEC-04 | IOMMU / DMA isolation. External devices get zero DMA until authorized. | [Technical Specification](technical_spec.md) §2.12 |
| SEC-05 | Verified boot chain: firmware → signed boot manifest → kernel → services → system image. | |
| SEC-06 | Immutable, read-only, Merkle-sealed system partition (A/B swap). | [Technical Specification](technical_spec.md) FS-01, Constitution §6 |
| SEC-07 | **Zero telemetry.** The kernel itself must not phone home, ever. | Constitution §7, [Technical Specification](technical_spec.md) §5 |
| SEC-08 | Protection domains carry resource budgets. Kernel enforces hard limits. | |

---

## 7. Interrupt & Device Model

| ID | Requirement | Source |
| --- | --- | --- |
| INT-01 | Minimal kernel interrupt handler → immediate dispatch to user-space driver thread. | |
| INT-02 | InterruptObject delivers hardware IRQs to user-space via Notification objects. | [Technical Specification](technical_spec.md) KRN-02 |
| INT-03 | Per-device DMA domains via IOMMU. | |
| INT-04 | APIC support (for SMP timer and inter-processor interrupts). | Roadmap §4, Phase 3 |

---

## 8. Power & Time

| ID | Requirement | Source |
| --- | --- | --- |
| PWR-01 | Abstract OS power states (not hardcoded to ACPI S-states). | |
| PWR-02 | Mechanism (kernel) / policy (user-space) split for power management. | |
| TIME-01 | Dynamic clocksource selection (TSC, paravirt, HPET). | |
| TIME-02 | vDSO for fast user-space clock reads. | |
| TIME-03 | Nanosecond resolution with explicit resolution reporting. Kernel is UTC-only. | |

---

## 9. Init / Supervisor Contract

| ID | Requirement | Source |
| --- | --- | --- |
| INIT-01 | The first user-space process is a supervisor tree (s6-inspired). | [Technical Specification](technical_spec.md) §2.6 |
| INIT-02 | Parallel startup of independent service branches. | [Technical Specification](technical_spec.md) KRN-06 |
| INIT-03 | Auto-restart of crashed services with backoff and escalation. | |
| INIT-04 | Structured, indexed, queryable logging. | [Technical Specification](technical_spec.md) KRN-07 |

---

## 10. Syscall Interface

| ID | Requirement | Source |
| --- | --- | --- |
| SYS-01 | ~30-50 syscalls total. Deliberately minimal. | |
| SYS-02 | Namespaced syscall identifiers. | |
| SYS-03 | Unstable ABI pre-v1.0, stable-versioned post-v1.0. | |
| SYS-04 | Machine-readable syscall documentation. | |

---

## 11. Compatibility Constraints on the Kernel

The kernel itself doesn't implement compatibility, but it must provide the *mechanisms* that user-space compatibility subsystems depend on.

| ID | Requirement | Why the Kernel Cares | Source |
| --- | --- | --- | --- |
| COMPAT-K1 | ELF loading capability. | The Linux compat subsystem needs to load ELF binaries. The kernel or a user-space loader must handle this. | [Technical Specification](technical_spec.md) COMPAT-02 |
| COMPAT-K2 | Syscall translation boundary. | The Linux compat layer translates Linux syscalls into native kernel IPC. The kernel must support a mechanism for this (e.g., a syscall proxy or trap-and-redirect). | [Technical Specification](technical_spec.md) COMPAT-02 |
| COMPAT-K3 | Pseudo-filesystem emulation support. | `/proc`, `/sys`, `/dev` must be emulable in user-space. The kernel's VFS (if any) or mount namespace must allow this. | |

---

## 12. Observability Requirements (for the Living View)

| ID | Requirement | Source |
| --- | --- | --- |
| OBS-01 | The kernel must expose real-time metrics for IPC traffic, scheduler state, memory pressure, interrupt rates, and capability events. | [Technical Specification](technical_spec.md) §2.19, UI-10 |
| OBS-02 | Kernel tracepoints (ftrace-like) for profiling and debugging. | |
| OBS-03 | Crash dumps: automatic, stored encrypted. | |
| OBS-04 | Panic screen: calm, dark, human-readable, auto-restart. | |

---

## 13. Size & Complexity Budget

| ID | Requirement | Source |
| --- | --- | --- |
| SIZE-01 | **Target: 50-100K LOC Rust** for the architecture-independent core. | [Technical Specification](technical_spec.md) KRN-03 |
| SIZE-02 | Designed for eventual formal verification of capability boundaries (not v1, but the code must be structured to enable it). | [Technical Specification](technical_spec.md) §2.1 |

---

## 14. What the Kernel Explicitly Does NOT Do

These are things that must live in user space. If a candidate kernel does them in-kernel, it violates our architecture.

| Excluded from Kernel | Where It Lives | Source |
| --- | --- | --- |
| Filesystem implementation | User-space filesystem server | Constitution §3, [Technical Specification](technical_spec.md) §2.7 |
| Network stack (TCP/IP, DNS, TLS) | User-space network server | [Technical Specification](technical_spec.md) §2.11 |
| Display protocol / compositor | User-space compositor | [Technical Specification](technical_spec.md) §2.8 |
| Audio mixing / routing | User-space audio server | [Technical Specification](technical_spec.md) §2.9 |
| Device drivers (GPU, NIC, USB, storage) | User-space driver processes | [Technical Specification](technical_spec.md) §2.1 |
| AI inference / knowledge graph | User-space AI service | [Technical Specification](technical_spec.md) §2.14, §2.15 |
| Application lifecycle / process model | User-space supervisor | [Technical Specification](technical_spec.md) §2.6 |
| Semantic typing of IPC messages | User-space IDL layer | [Technical Specification](technical_spec.md) INV-03 |
| Broadcasting / pub-sub | User-space event broker | |
| Power management policy | User-space power manager | |

---

## Summary: The Kernel in One Paragraph

Our kernel is a **minimal, Rust-only, capability-based microkernel** targeting x86_64 UEFI systems. It manages exactly 10 kernel objects (Thread, AddressSpace, CapabilitySpace, Endpoint, Notification, MemoryObject, Mapping, InterruptObject, TimerObject, SchedulingContext) through ~30-50 syscalls. It provides sub-microsecond IPC, a multi-class scheduler (Deadline through Idle), IOMMU-enforced device isolation, and ASLR/KASLR. It does **nothing else**. No drivers, no filesystems, no networking, no AI, no display — all of that is user-space services communicating over the kernel's IPC. The kernel is designed to be formally verifiable, and its total architecture-independent codebase targets 50-100K lines of Rust.

---

## Evaluation Framework for Candidates

Use this checklist to evaluate any existing kernel (fork candidate) or any "build from scratch" plan:

| Category | Weight | Question |
| --- | --- | --- |
| **Architecture** | Critical | Is it a true microkernel? (Drivers, FS, net in user-space?) |
| **Language** | Critical | Is it Rust? If not, how much rewrite is needed? |
| **Capability System** | Critical | Does it use capability-based access control natively? |
| **IPC Model** | Critical | Does it support sync call/reply + async notification + shared memory + capability passing? |
| **Fault Isolation** | Critical | Can user-space services crash without kernel impact? |
| **Scheduler** | High | Does it support multiple scheduling classes (especially Deadline)? |
| **IOMMU** | High | Does it support per-device DMA isolation? |
| **Observability** | Medium | Does it expose runtime metrics and tracepoints? |
| **Size/Complexity** | Medium | How large is the codebase? Can it be understood by a small team? |
| **Community/Docs** | Medium | Is there documentation? Active development? Can you get help? |
| **License** | Critical | Is it MIT/Apache-2.0 compatible? |
