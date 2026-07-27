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

**v0.9 — VirtIO Reference Platform, Storage, Networking & SMP.** Tagged `v0.9.5`. Completed full VirtIO virtual hardware reference platform (Block, Net, GPU, Input), GaxFS native storage engine with Copy-on-Write dual superblocks & TurboQuant SIMD vector search, GaxNet Ring-3 protocol stack (TCP/UDP/IP/DNS/TLS) with zero-copy `PacketRing`, and 64-core SMP inter-core scheduling.

### Pre-v1.0 Audit & Hardening Series

A 5-phase zero-compromise audit and microkernel verification program preparing the system for formal `v1.0.0` release:

* **Phase 1 (GaxNet & Ring-3 Server Hardening):** Protocol state machine verification, dynamic AEAD sequence nonces, and `_start` event loops across all 8 Ring-3 services.
* **Phase 2 (Memory Foundation & Kernel Bounds):** Frame allocator double-free prevention, W^X PML4 unmapping, SMAP `copy_from_user` safety, and canonical address bounds.
* **Phase 3 (Capability Engine & Revocation Matrix):** Rights attenuation matrix validation, 3-tier cascade revocation, and object generation handle validation.
* **Phase 4 (IPC & Multi-Core Scheduler Audit):** Multi-client `WaitSet` event multiplexing, lock-rank ordering, and 64-core scheduler domain topology load balancing.
* **Phase 5 (Full System Integration Audit):** Endpoint privilege enforcement, sequence-XOR reply tokens, constant-time Poly1305 MAC checks, zero-clippy workspace verification, and bootable UEFI ISO packaging.

### Audit Completion & Hardening Freeze

The Pre-v1.0 Audit & Hardening program has formally concluded with the verification of Phase 5. The core kernel ABI, memory model, capability matrix, and IPC invariants are locked. Any further minor fixes or refactorings will land in post-v1.0 maintenance updates.

### Road to Formal v1.0.0 Release

With pre-v1.0 verification complete, Gaxera is transitioning to its formal **v1.0.0 Release**. This upcoming landmark milestone will establish the official v1.0.0 specification, expanded architectural documentation, and an extensive post-v1.0 vision and roadmap.

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
