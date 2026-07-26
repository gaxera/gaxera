# Gaxera

> What would an operating system look like if we designed it today
> — from nothing — knowing everything we know now?

Not a fork. Not a distribution. Not another layer on Linux.
A ground-up answer.

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE-MIT)

## Why Start Over

Every major operating system carries decades of compromises made for
a world that no longer exists. Security bolted on after the fact. File
systems that store bytes but lose every shred of context. AI treated
as a product feature instead of a fundamental reality. Privacy as a
settings toggle instead of a structural guarantee.

These aren't bugs. They're consequences of building on foundations
that were never designed for what computing became.

Gaxera starts from a different premise: keep the lessons. Drop the
constraints. Design what should exist, not what's easiest to patch.

## What Comes Out Different

**Security is the kernel, not a layer on it.** Every resource requires
an unforgeable capability token. No token, no access. No root. No
sudo. No ambient authority. This is how the kernel itself works — not
something wrapped around it.

**AI is infrastructure you control.** Not a chatbot in a sidebar. Not
a feature you subscribe to. Intelligence embedded across the system —
resource management, workflow understanding, system adaptation. You
wield it. It doesn't wield you.

**Knowledge, not just files.** Traditional file systems are glorified
containers — they hold bytes and forget everything else. Why a
decision was made, how components relate, the history of a project's
evolution — all of it lost the moment you hit save. Gaxera's data
model preserves context: reasoning, relationships, and lineage travel
with the data. The system understands what it holds, not just where
it's stored.

**Compatibility without compromise.** Gaxera is designed as if Windows,
Linux, and macOS never existed. Then it builds translation layers that
run existing software on its own terms. Legacy shapes the compatibility
layer — never the architecture.

**Privacy by structure.** Zero telemetry isn't a toggle in settings.
It's a property of the architecture itself. The pathways for data to
leak simply don't exist, because they were never built.

## Status

**v0.1 — Foundation release.** Tagged `v0.1.0` and `phase-6-complete` at `f6b2146`; validated by the deterministic UEFI QEMU matrix.

**v0.5 — Capabilities & Microkernel Program.** Tagged `v0.5.0` at `e7f89ab`. Implemented capability derivation/revocation state machines, ring-3 privilege transitions, fault-recoverable syscall ABI (`copy_from_user`), preemptive thread scheduler, core IPC, and `ramfs` supervisor.

**v0.6 — Core Memory Foundation.** Tagged `v0.6.0` at `2ccd6fc`. Implemented physical frame recycling, recursive PML4 page-table reclamation (ADR 0018), O(1) typed `SlabCache<T>` allocators with dynamic heap growth (ADR 0019), subregion memory mapping, and `UnmapMemory` opcode with TLB flushing (ADR 0020).

**v0.7 — Multi-Client IPC & Event Multiplexing.** Tagged `v0.7.0`. Epoch 2 evolves IPC from 1:1 rendezvous into a high-performance $N:1$ multi-client server architecture.

**v0.8 — Capability Microkernel Release.** Tagged `v0.8.0`. Implemented generation-tracked object arena, recursive capability revocation (ADR 0007, 0021), hardware IRQ delegation (ADR 0022), MMIO driver mapping (ADR 0023), type-safe `libgaxera` runtime (ADR 0024), user-space `init` service registry (ADR 0025), and direct context-switch IPC fast-paths (ADR 0026).

**v0.9 — Hardware Enablement, Storage, Networking & VirtIO Reference Platform.** Tagged `v0.9.5`. Complete VirtIO reference platform (Block, Net, GPU, Input), native storage engine, SIMD vector index, query architecture, and GaxNet native network platform:

* **Milestone 0.9.0 (PCIe ECAM & PCI Bus Server):** ACPI MCFG table parsing, ECAM capability generation, and user-space PCI bus scanner.
* **Milestone 0.9.1-recovery (Architectural Recovery Baseline):** DMA ContiguousFrame objects, W^X loader checks, and 5-level RankedLock hierarchy.
* **Milestone 0.9.2 (SMP Load Balancing & Inter-Core Scheduling):** CpuAffinityMask, SchedulerDomain topology controller, and inter-core work stealing across 64 CPUs.
* **Milestone 0.9.3 (GaxFS Native Storage Platform):** RFC 9562 UUIDv7 object identity, Copy-on-Write dual-superblock commit engine (`gax_storage_engine`), shared-memory event stream (`gaxfs_event_log`), TurboQuant FWHT 4-bit Lloyd-Max quantization with RaBitQ scale correction & SIMD capability isolation (`gaxfs_vector_index`), and GaxQL 3-layer query planner (`query_planner`).
* **Milestone 0.9.4 (VirtIO Network Server & GaxNet Native Network Platform):** First-principles capability-native networking architecture (`ADR 0035`), unprivileged Ring-3 `virtio_net_server` driver, `net_stack_server` protocol engine (Ethernet II, ARP cache, IPv4/IPv6 router, ICMP ping, UDP socket table, stateful TCP with RFC 793 state machine, FIN teardown, NewReno 3-dupACK Fast Retransmit/Recovery, and RFC 6298 dynamic RTO retransmission timers), `NetNamespace` isolation, decoupled `resolver_server` (RFC 1035 wire DNS query encoder & IPv4 answer parser) and `crypto_server` (ChaCha20-Poly1305 AEAD session encryption & MAC tag verification), `PacketRing` shared memory data plane, 6 layered provider traits, and POSIX BSD Sockets Virtualization Layer (`libgaxera::compat::sockets`).
* **Milestone 0.9.5 (Complete VirtIO Reference Platform):** Ring-3 `virtio_gpu_server` (2D display scanout, RGBA color bar framebuffer rendering, Virtqueue command rings) and `virtio_input_server` (keyboard & pointer event decoding, `FocusHandle` capability scoping, zero keylogging) completing the 100% VirtIO virtual hardware reference platform on QEMU (`ADR 0036`).

**Pre-v1.0 Audit & Hardening Series.** Tagged `pre-v1.0-phase-1`. A 5-phase comprehensive pre-v1.0 audit, protocol hardening, and microkernel verification program preparing the system for formal `v1.0.0` release:

* **Phase 1 (GaxNet & Ring-3 Server Hardening) [COMPLETED]:** Tagged `pre-v1.0-phase-1`. Hardened `net_stack_server` (RFC 793 TCP state machine, FIN teardown, NewReno 3-dupACK Fast Retransmit/Recovery, RFC 6298 RTO timers, UDP socket tables), `crypto_server` (ChaCha20-Poly1305 AEAD payload encryption), `resolver_server` (RFC 1035 wire DNS query encoder & IPv4 answer parser), initialized active `_start` IPC event loops across all 8 Ring-3 server binaries, and verified 100% test coverage with raw execution evidence logs across checkpoints 15–20.
* **Phases 2–5 (Kernel, Memory, IPC & Compatibility Audit) [IN PROGRESS]:** System-wide verification and hardening across memory recycling, capability revocation, multi-client IPC multiplexing, and compatibility layers.

**Current Status & Next Steps:** Following Milestone 0.9.5 and Pre-v1.0 Phase 1, Gaxera is progressing through its **5-phase pre-v1.0 audit and hardening cycle** (Phases 2 through 5). This program establishes absolute codebase integrity across all kernel, storage, memory, IPC, and compatibility layers, preparing the platform for its formal `v1.0.0` release and laying a rock-solid foundation for post-v1.0 bare-metal hardware drivers.

Detailed milestones are tracked in [v0.1 Roadmap](docs/roadmap/roadmap_v01.md), [v0.5 Roadmap](docs/roadmap/roadmap_v05.md), [v0.6 Roadmap](docs/roadmap/roadmap_v06.md), [v0.7 Roadmap](docs/roadmap/roadmap_v08.md), [v0.8 Roadmap](docs/roadmap/roadmap_v08.md), and [v0.9 Roadmap](docs/roadmap/roadmap_v09.md).
The exact architecture and methodology are documented in the [Engineering Workflow Reference](.internal/Engineering%20Workflow.md), [Foundation v0.1 Reference](docs/architecture/foundation_v0.1.md), [Memory Architecture Reference](docs/architecture/memory.md), [IPC Architecture Reference](docs/architecture/ipc.md), [GaxFS Master Architecture](docs/architecture/gaxfs_master_architecture.md), [GaxNet Master Specification](docs/architecture/gaxnet_specification.md), [VirtIO Platform Specification](docs/architecture/virtio_reference_platform.md), and [ADRs 0000–0036](docs/adr/).

## Getting Started

Refer to the [Developer Workflow Guide](docs/development/workflow.md) for instructions on bootstrapping the toolchain, building the kernel hybrid ISO, running Gaxera in QEMU, and executing the verification test suite.

## Contributing

I'm the only one building this right now.
[CONTRIBUTING.md](CONTRIBUTING.md) if you want to change that.

Architectural decisions go through a formal
[ADR process](docs/adr/0000-engineering-philosophy.md). I'd rather
be slow and right than fast and lost.

## License

Dual-licensed under [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE).
