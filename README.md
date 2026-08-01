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

**v0.9 — VirtIO Reference Platform, Storage, Networking & SMP Baseline.** Tagged `v0.9.5`. Delivered complete VirtIO virtual hardware reference platform (Block, Net, GPU, Input), GaxFS Copy-on-Write dual superblock storage engine prototype, GaxNet Ring-3 protocol stack with RFC 8439 ChaCha20-Poly1305 AEAD payload encryption (`crypto_server`), zero-copy `PacketRing` buffers, and 64-core SMP scheduler domain topology.

**v1.0.0 — Formal Architectural Baseline.** Tagged `v1.0.0`. Locked the capability microkernel contracts, documented subsystem maturity boundaries, and established the QEMU UEFI reference platform as the supported verification environment. Storage, networking, physical SMP, and hardware-driver layers remain explicitly classified as prototypes or reference implementations where noted below.

**v1.1.0 — Ring-3 Memory Foundation.** The current release candidate adds ResourceDomain byte quotas, Factory type authorization, page-rounded zeroed anonymous MemoryObjects, narrow capability rights, transactional frame rollback, mapping lineage, reference-class reclamation, and a fallible `libgaxera::UserspaceAllocator` verified by genuine Ring-3 QEMU tests. Hardware IRQ delivery remains an architecture draft, and production-service allocator migration is deferred until the process bootstrap capability contract is defined.

### Pre-v1.0 Audit & Hardening Series

A 5-phase audit and microkernel verification program preparing the system for formal `v1.0.0` release:

* **Phase 1 (GaxNet & Ring-3 Server Hardening):** Protocol state machine verification, dynamic AEAD sequence nonces, constant-time Poly1305 MAC tag checks, and active `_start` event loops across all 8 Ring-3 services.
* **Phase 2 (Memory Foundation & Kernel Bounds):** Frame allocator double-free prevention, W^X PML4 unmapping, SMAP `copy_from_user` safety, and canonical address bounds.
* **Phase 3 (Capability Engine & Revocation Matrix):** Rights attenuation matrix validation, 3-tier cascade revocation, and object generation handle validation.
* **Phase 4 (IPC & Multi-Core Scheduler Audit):** Multi-client `WaitSet` event multiplexing, lock-rank ordering, and 64-core scheduler domain topology load balancing.
* **Phase 5 (Full System Integration Audit):** Endpoint privilege enforcement, sequence-XOR reply tokens, zero-clippy workspace verification, and bootable UEFI ISO packaging.

### Subsystem Readiness Classification (v1.0.0)

Gaxera `v1.0.0` explicitly classifies subsystem maturity levels:

* **Stable Architectural Baseline (Locked for v1.0.0):** Microkernel ABI, memory manager & PML4 reclamation, capability derivation/attenuation/revocation, direct-switch synchronous IPC, and 11 core kernel objects.
* **Reference Platform (Verified in QEMU UEFI):** Single-core (BSP) VirtIO virtual hardware drivers (`virtio_block`, `virtio_net`, `virtio_gpu`, `virtio_input`) running in Ring 3 process isolation.
* **Prototypes (Active Architecture / Scheduled for Post-v1 Initiatives):**
  * *GaxFS Storage:* Copy-on-Write dual-superblock engine prototype using non-cryptographic rolling checksums in `integrity.rs` and scalar vector search. Durable root reconstruction, BLAKE3 cryptographic integrity, and SIMD optimization are scheduled for Initiative `v1.3`.
  * *GaxNet & Security:* Protocol engine and RFC 8439 ChaCha20-Poly1305 AEAD payload encryption. Transmit frames are queued on local `PacketRing` slots, DNS answers are synthesized locally, and full TLS 1.3 handshake negotiation is scheduled for Initiative `v1.4`.
  * *SMP & Execution:* 64-core scheduler domain model and work-stealing algorithms tested via host unit tests; AP bring-up and ICR IPI delivery are simulated (BSP execution). Physical AP bring-up is scheduled for Initiative `v1.2`.
  * *Driver Isolation:* Ring 3 drivers operate in process isolation; bus-mastering drivers function as trusted drivers. Hardware DMA isolation via IOMMU (VT-d/AMD-Vi) is scheduled for Initiative `v1.5`.
  * *Userspace Heap & Locking:* The dedicated Ring-3 heap test service now uses the fallible `MemoryObject`-backed allocator and passes quota, fragmentation, and post-OOM recovery checks. Existing production service entrypoints still use static layouts or the legacy dummy allocator until a generic startup capability contract exists. `RankedLock` rank checking remains `#[cfg(test)]`-gated and is a later SMP hardening item.
* **Future Architecture:** Physical bare-metal drivers, GaxView native compositor, GaxCompat translation layers, and AI metadata infrastructure.

### Release Road

The formal **v1.0.0 release is complete**. The current v1.1.0 release candidate extends that baseline with the Ring-3 memory foundation while keeping hardware IRQ delivery and production-service migration as separately gated work.

Detailed milestones are tracked in [v0.1 Roadmap](docs/roadmap/roadmap_v01.md), [v0.5 Roadmap](docs/roadmap/roadmap_v05.md), [v0.6 Roadmap](docs/roadmap/roadmap_v06.md), [v0.7 Roadmap](docs/roadmap/roadmap_v07.md), [v0.8 Roadmap](docs/roadmap/roadmap_v08.md), [v0.9 Roadmap](docs/roadmap/roadmap_v09.md), and the [v1.1 Roadmap](docs/roadmap/roadmap_v11.md).
The exact architecture and methodology are documented in the [Developer Workflow Guide](docs/development/workflow.md), [Technical Specification](docs/spec/technical_spec.md), [Foundation v0.1 Reference](docs/architecture/foundation_v0.1.md), [Memory Architecture Reference](docs/architecture/memory.md), [IPC Architecture Reference](docs/architecture/ipc.md), [GaxFS Architecture](docs/architecture/gaxfs_specification.md), [GaxNet Master Specification](docs/architecture/gaxnet_specification.md), [VirtIO Platform Specification](docs/architecture/virtio_reference_platform.md), and the [ADR index](docs/adr/).

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
