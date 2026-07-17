# Technical Specification

> **Status:** Canonical | **Version:** 1.1 | **Last Updated:** 2026-07-17
> **Related:** [Roadmap](../roadmap/roadmap_v01.md), [Constitution](../governance/constitution.md)

**Purpose:** Canonical intended architecture reference.
**Implementation State:** The architecture described here is the *intended*
long-term system architecture. It is not the current implementation state.
Foundation v0.1 is released; v0.5 implementation is governed by the frozen
`roadmap_v05.md` program and `architecture/v05_requirements_trace.md`.

## 0. Status Taxonomy

Architectural statements use the following semantic distinctions:

- **[COMMITTED]:** An architectural direction or design constraint deliberately selected. Changing it requires an ADR.
- **[REQUIREMENT]:** A behavior or property that the implementation must satisfy, without prescribing the exact mechanism.
- **[TARGET]:** A measurable desired outcome requiring benchmarking or measurement (not a commitment merely because it is intended).
- **[HYPOTHESIS]:** An assumption or prediction that requires empirical validation.
- **[RESEARCH REQUIRED]:** An unresolved architectural or technical question that must be investigated before implementing the affected component.
- **[DEFERRED]:** A valid requirement or feature intentionally postponed to a later milestone or phase.
- **[ASPIRATIONAL]:** A long-range direction or ambition that is not currently an implementation commitment.

## 1. Architectural Invariants

The following properties must hold and cannot be casually violated by future implementation:

- **INV-01 [COMMITTED]:** User-space service failure must not imply kernel failure.
- **INV-02 [COMMITTED]:** System state and user data remain architecturally separable.
- **INV-03 [COMMITTED]:** Kernel IPC transports mechanism, not application semantics.
- **INV-04 [COMMITTED]:** The knowledge graph must never be on the synchronous critical path of ordinary file I/O.
- **INV-05 [COMMITTED]:** Compatibility subsystems do not automatically inherit unrestricted host authority.
- **INV-06 [COMMITTED]:** AI tool execution goes through an authorization boundary.
- **INV-07 [COMMITTED]:** Irreversible or high-impact AI actions require explicit confirmation according to the Tool Broker policy.

## 2. Component Specifications

### 2.1 Microkernel

**Decision:** A minimal ring-0 kernel managing threads, address spaces, IPC, capabilities, interrupts, and page-level memory. Everything else lives in user space.
**Rationale:** Minimizes the Trusted Computing Base (TCB) and isolates faults to user-space services.
**Trade-offs:** User-space drivers and services increase IPC overhead compared to monolithic kernels.
**Dependencies:** UEFI bootloader handoff.
**Risks:** Achieving acceptable IPC latency on consumer hardware.
**Validation:** QEMU integration tests, benchmark suites, and eventually formal verification of capability boundaries.

- **KRN-01 [COMMITTED]:** Source: S6. Language: Rust + minimal assembly. Validation: Compiles via `cargo build` on `x86_64-unknown-none`.
- **KRN-02 [COMMITTED]:** Amended by ADR 0008. 11 kernel objects: Thread,
  AddressSpace, CapabilitySpace, Endpoint, Notification, MemoryObject,
  Mapping, InterruptObject, TimerObject, SchedulingContext, and
  ResourceDomain. A Factory is authority on a ResourceDomain, not an object.
- **KRN-03 [TARGET]:** Source: S6. Size budget: 50–100K LOC Rust for architecture-independent core. Validation: Line count metrics.

### 2.2 Scheduler

**Decision:** Multi-class scheduler supporting real-time, interactive, throughput, and background workloads.
**Rationale:** Required to support real-time audio and compositor needs alongside heavy background AI inference.
**Dependencies:** Timer interrupts and kernel threading.

- **KRN-04 [COMMITTED]:** Source: S6. Scheduling classes: Deadline, Fixed-Priority RT, Interactive Fair (EEVDF), Throughput Fair, Maintenance/Idle.
- **KRN-05 [HYPOTHESIS]:** Source: S6. Foreground focus acts as a scheduling hint, not a blanket priority boost. Validation: UX responsiveness benchmarking under heavy load.

### 2.3 Memory Manager

**Decision:** Virtual memory, paging, address space isolation, and hierarchical pressure management.
**Rationale:** Prevents OOM panics through proactive lifecycle management and tiered storage.

- **MEM-01 [COMMITTED]:** Source: S6. ASLR/KASLR enabled by default.
- **MEM-02 [COMMITTED]:** Source: S6. 8-step OOM sequence with lifecycle-aware scoring and emergency reserve.
- **MEM-03 [TARGET]:** Source: S18, S53. Base OS idle RAM < 1 GB. (Stretch target: < 500 MB without AI).
- **MEM-04 [TARGET]:** Source: S48. AI-active memory budget: ~2 GB to 5 GB depending on model size.
- **MEM-05 [COMMITTED]:** Amended by ADR 0008. After mandatory bootstrap,
  user-triggerable allocation is fallible and resource exhaustion must not
  panic the kernel. ResourceDomain owns the initial bounded accounting model.

### 2.4 IPC (Inter-Process Communication)

**Decision:** Asynchronous and synchronous message passing; shared memory for bulk data.
**Rationale:** The nervous system of the microkernel; performance dictates overall OS viability.

- **IPC-01 [COMMITTED]:** Source: S6. Kernel transports bytes + capability refs; semantic typing is handled in user-space IDL.
- **IPC-02 [TARGET]:** Source: S6. Sub-microsecond same-core round trips on reference hardware. Validation: Ping-pong IPC benchmarks.

### 2.5 Capability System

**Decision:** All access control is capability-based. No ambient authority. No root.
**Rationale:** Structural defense against confused deputy attacks and privilege escalation.
**Dependencies:** IPC for capability passing.

- **SEC-01 [COMMITTED]:** Source: S12. No root/sudo. Scoped capabilities only.
- **SEC-02 [COMMITTED]:** Source: S12. The capability system supports explicit revocation and time-bounded leases.
- **SEC-03 [REQUIREMENT]:** Automatic expiry of unused sensitive permissions as a policy-driven user-space mechanism.

### 2.6 Init / Supervisor

**Decision:** The first user-space process acts as a supervisor tree (s6-inspired).
**Rationale:** Ensures deterministic service startup, dependency management, and auto-restarting of crashed user-space drivers.

- **KRN-06 [COMMITTED]:** Source: S6. Parallel startup of independent branches.
- **KRN-07 [COMMITTED]:** Source: S10. Structured logging, indexed and queryable.

### 2.7 Filesystem (Custom CoW)

**Decision:** Copy-on-write semantics enabling snapshots, version history, and safe power-cut recovery, coupled with a knowledge system for provenance and semantic relationships.
**Rationale:** Foundational for time travel, robust atomic updates, and intent-based computing.
**Trade-offs:** Decoupling the semantic graph from the filesystem inode level increases latency for complex queries, while coupling it tightly limits compatibility and increases implementation complexity.

- **FS-01 [COMMITTED]:** Source: S4, S6. Three logical areas: EFI (FAT32), System (read-only, Merkle-sealed, A/B), User Data (CoW, writable).
- **FS-02 [COMMITTED]:** Source: S4. Snapshots provide the foundation for time travel and rollback.
- **FS-03 [TARGET]:** Source: S4. Transparent compression (zstd) with 1.5-2x general ratio.
- **FS-04 [RESEARCH REQUIRED]:** Source: S4. Evaluate whether to build/adapt a CoW filesystem (e.g. bcachefs) and how tightly the semantic graph storage should couple to filesystem internals (native metadata, xattrs, or separate store).

### 2.8 Graphics & Display Stack

**Decision:** Custom display protocol, Vulkan-based compositor, GPU-accelerated vector rendering.
**Rationale:** Must map cleanly onto our capability-mediated IPC and explicit synchronization requirements. The exact architectural fit with existing ecosystems remains under research.

- **UI-01 [COMMITTED]:** Source: S2. Integrated display server/compositor using zero-copy shared buffers.
- **UI-02 [RESEARCH REQUIRED]:** Source: S2. Scope of custom display protocol versus a capability-aware adaptation of Wayland concepts.
- **UI-03 [RESEARCH REQUIRED]:** Source: S21. Hardware decoding pipeline and distribution/licensing obligations for codecs.

### 2.9 Audio Engine

**Decision:** Custom audio server with PipeWire-inspired graph design.
**Rationale:** User-space, capability-gated, and low-latency.

- **UI-04 [RESEARCH REQUIRED]:** Source: S21. Custom Bluetooth stack feasibility and scope.

### 2.10 Input System

**Decision:** Unified event model from hardware to app.
**Rationale:** Centralizes input handling for consistent accessibility and hotplug support.

- **UI-05 [COMMITTED]:** Source: S9. High poll rate, configurable acceleration, accessibility hooks.

### 2.11 Networking

**Decision:** Full TCP/IP stack, DNS, TLS, WiFi — all in user space.
**Rationale:** Isolates network stack vulnerabilities from the kernel.

- **SEC-04 [COMMITTED]:** Source: S13, S24. Per-app network capability acts as the firewall (default-deny).
- **SEC-05 [COMMITTED]:** Source: S24. Encrypted DNS (DoH/DoT) by default.

### 2.12 Security Architecture

**Decision:** Defense in depth.
**Rationale:** Security requires multiple layers; no single mechanism (including AI) is a silver bullet.
**Dependencies:** Capability system, microkernel isolation, IOMMU.

- **SEC-06 [COMMITTED]:** Source: S12. Defense in depth layers: capability-based isolation, no ambient authority, minimal TCB, immutable verified system state, sandboxed apps, IOMMU/DMA isolation, snapshots/recovery, and behavioral detection as an *additional* defensive layer.
- **SEC-07 [COMMITTED]:** Source: S12. TPM-anchored biometrics and FIDO2/WebAuthn.

### 2.13 Compatibility Subsystems

**Decision:** Run apps from other OSs through isolated per-platform subsystems.
**Rationale:** Bootstraps the app ecosystem while maintaining native security and isolation. The current architectural strategy hosts Wine on top of a Linux-compatible substrate, which makes Linux compatibility an implementation prerequisite for Windows compatibility.

- **COMPAT-01 [REQUIREMENT]:** **Native Runtime:** Native binaries and native APIs.
- **COMPAT-02 [REQUIREMENT]:** **Linux Compatibility Environment:** ELF loading, ABI/syscall translation, pseudo-filesystem compatibility, D-Bus bridging, Linux GUI protocol bridges, and device access bridging through native capabilities.
- **COMPAT-03 [REQUIREMENT]:** **Windows Compatibility Environment:** Wine user-space components, PE handling, Win32 API implementation, registry environment, COM compatibility, and DirectX translation using Vulkan translation layers running on the Linux-compatible substrate.
- **COMPAT-04 [HYPOTHESIS]:** Source: S49. Linux app overhead 5-10%; Windows app overhead 5-15% CPU. Validation: Real-world benchmarking against native Linux/Windows.

### 2.14 AI Architecture

**Decision:** Local-first, tiered AI pipeline operating as a privilege-separated user-space service.
**Rationale:** Protects user privacy while providing OS-level intelligence.

- **AI-01 [COMMITTED]:** Source: S3, S48. Tiered pipeline: L1 (rule-based) -> L2 (router) -> L3 (specialist) -> L4 (LLM).
- **AI-02 [RESEARCH REQUIRED]:** Source: S48. AI memory and battery life impact estimates under sustained load.
- **AI-03 [COMMITTED]:** Source: S24. Zero telemetry. AI learning is strictly local and user-deletable.

### 2.15 Knowledge System

**Decision:** Graph-based semantic layer tracking files, relationships, intents, and time.
**Rationale:** Enables intent-based computing and semantic search without brittle path dependencies.

- **KNOW-01 [COMMITTED]:** Source: S35. Provenance (source/timestamp/creation-method) attached to every node and edge.

### 2.16 UI Toolkit

**Decision:** System-wide GPU-accelerated declarative UI framework (fork of Iced).
**Rationale:** Ensures consistent performance, accessibility, and design tokens across all native apps.

- **UI-06 [COMMITTED]:** Source: S5, S26. Rendering via wgpu/Vulkan (Vello).
- **UI-07 [RESEARCH REQUIRED]:** Source: S55. Bidirectional text handling: clarify the distinction between text shaping (rustybuzz) and the Unicode Bidirectional Algorithm implementation.

### 2.17 Shell & Terminal

**Decision:** Custom shell with POSIX compatibility and modern interactive UX.
**Rationale:** Provides power-user efficiency and seamless integration with the compat subsystem.

### 2.18 Package Manager

**Decision:** Native app distribution using a custom sandboxed format.
**Rationale:** Clean installation/removal, capability manifest enforcement, and no registry rot.

- **COMPAT-05 [COMMITTED]:** Source: S15, S53. Nix-style content-addressed store for shared libraries to prevent version conflicts.

### 2.19 System Observability & The Living View

**Decision:** System observability is split into two distinct, strictly truthful paradigms: The Engineering Monitor (mechanical truth) and The Living View (biological/organic semantic truth).
**Rationale:** Humans naturally understand organic states (stress, flow, attention, dormancy) better than raw hex addresses and process tables. The OS is represented as a living organism to intuitively visualize the cognitive and coordination planes.
**Trade-offs:** Rendering a real-time biological visualizer requires GPU overhead and tight coupling with kernel metrics to ensure it is not just a decorative skin, but a truthful reflection of system state.

- **UI-08 [COMMITTED]:** **The Living View:** A real-time, explorable, living representation of the OS. The Brain represents the Cognitive and Coordination Plane (AI, intent routing, semantic memory). The Nervous System represents IPC. The Heart represents the scheduler/clock. The Immune System represents capability/behavioral security.
- **UI-09 [COMMITTED]:** **The Engineering Monitor:** A traditional observability suite (processes, PIDs, memory pages, IPC channels, thread counts) for technical inspection.
- **UI-10 [REQUIREMENT]:** Both views share the exact same underlying source of truth. A glowing "nerve" in the Living View must correspond to a real, measurable IPC channel traffic spike in the Engineering Monitor.

## 3. Performance & Operational Targets

| Target ID | Area | Status | Target Value | Validation Method |
| --- | --- | --- | --- | --- |
| PERF-01 | Boot | [HYPOTHESIS] | Power-on to shell: ~5s | Hardware profiling |
| PERF-02 | Boot | [TARGET] | Kernel to shell: ~3s | QEMU timestamps |
| PERF-03 | IPC | [HYPOTHESIS] | Same-core round trip: < 1μs | Ping-pong benchmark |
| PERF-04 | RAM | [TARGET] | Base OS idle: < 1 GB | Memory profiler |
| PERF-05 | UI | [TARGET] | Input-to-photon: < 10ms | High-speed camera test |

## 4. Research Debt Register

This register tracks unresolved architectural questions that block or influence future implementation. These must be resolved before implementing the affected components.

| ID | Question | Why It Matters | Affected Component | Resolution Output |
| --- | --- | --- | --- | --- |
| RD-01 | **Bidirectional text handling:** How does the UI toolkit correctly implement the Unicode Bidirectional Algorithm alongside text shaping? | Rendering Arabic/Hebrew text correctly is a fundamental OS requirement. | UI Toolkit | Prototype & ADR |
| RD-02 | **Broad filesystem compatibility:** How feasible is safe, performant read/write support for APFS/HFS+ without violating licensing or relying on unstable reverse engineering? | Affects cross-platform data migration promises. | Filesystem | Legal/Technical Analysis |
| RD-03 | **Codec licensing:** Does utilizing hardware decoding for H.264/H.265 trigger distribution or licensing obligations for the OS? | Legal liability and open-source compliance. | Graphics / Video | Legal Analysis |
| RD-04 | **Custom Bluetooth stack:** Is building a custom Bluetooth stack feasible for v1.0, or should BlueZ be adapted in user space? | Bluetooth complexity is notorious; building from scratch may derail the timeline. | Networking | Feasibility Report |
| RD-05 | **CoW Filesystem vs Semantic Graph:** Whether to build/adapt a CoW filesystem (e.g. bcachefs) and how tightly the semantic graph storage should couple to filesystem internals (native metadata, xattrs, separate store). | Building a production-grade CoW FS is a multi-year effort, and premature coupling of the knowledge graph limits flexibility. | Filesystem / Knowledge | ADR |
| RD-06 | **Display Protocol Architecture:** Should we build a fully custom native protocol, a capability-aware Wayland adaptation, or a hybrid bridge? How do their object models, IPC, and synchronization align with our security goals? | Reinventing the display protocol breaks all existing Linux GUI tools, but Wayland's defaults may not fit our capability model. | Compositor | Architectural Comparison |
| RD-07 | **Compat Overhead:** Are the 5-10% CPU overhead targets for Windows/Linux compatibility subsystems realistic under our specific microkernel IPC constraints? | User experience depends heavily on acceptable compat performance. | Compat Subsystems | Benchmark Results |
| RD-08 | **AI Battery/Memory:** What is the real-world battery drain and memory pressure of running tiered AI models (1B to 7B parameters) continuously? | Might require fundamentally changing the "always available" AI design. | AI Architecture | Benchmark Results |
| RD-09 | **IPC Latency:** Can a Rust microkernel achieve <1μs IPC latency while enforcing capability checks? | If IPC is slow, the entire user-space driver architecture fails. | Microkernel | Microbenchmark |

## 5. Cross-Cutting Concerns

### Privacy Guarantees (Constitutional)

- **ZERO telemetry.** Not opt-out. Non-existent.
- **100% offline capable** by construction.
- **System-wide incognito mode:** Suspends knowledge graph, history, and AI learning.

### Update Model

- **A/B partition swap** (ChromeOS/Silverblue model).
- **Delta updates** (OSTree-style binary diffing).
- **Atomic rollback** with "boot-successful" confirmation flag.

### Licensing

- **Dual MIT/Apache-2.0** (Rust ecosystem convention, patent grant).
- **Wine LGPL:** Cleanly separated component in the Linux subsystem.
