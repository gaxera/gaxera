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

**v0.7 — Multi-Client IPC & Event Multiplexing.** Tagged `v0.7.0`. Epoch 2 evolves IPC from 1:1 rendezvous into a high-performance $N:1$ multi-client server architecture:

* **Milestone 0.7.1 (Multi-Client Endpoint Call Queueing & Bounded Caller Waiting):** Expanded `Endpoint` state machine to queue up to 32 caller threads in FIFO order with clean thread cancellation (`cancel_caller`) and server crash recovery (`ADR 0021`).
* **Milestone 0.7.2 (First-Class `WaitSet` Kernel Object & Event Multiplexing):** Introduced `ObjectType::WaitSet = 13` and ABI opcodes (`CreateWaitSet`, `WaitSetControl`, `WaitSetWait`), allowing servers to block on multiple endpoints, notifications, and timers atomically without fast-path heap allocations (`ADR 0021`).
* **Milestone 0.7.3 (IPC Priority Inheritance & Fast-Path Optimization):** Added `base_priority` and `effective_priority` tracking to `Thread` to dynamically boost server thread priority to match high-priority callers during IPC rendezvous, preventing priority inversion.

Detailed milestones are tracked in [v0.1 Roadmap](docs/roadmap/roadmap_v01.md), [v0.5 Roadmap](docs/roadmap/roadmap_v05.md), [v0.6 Roadmap](docs/roadmap/roadmap_v06.md), and [v0.7 Roadmap](docs/roadmap/roadmap_v07.md).
The exact architecture and methodology are documented in the [Engineering Workflow Reference](.internal/Engineering%20Workflow.md), [Foundation v0.1 Reference](docs/architecture/foundation_v0.1.md), [Memory Architecture Reference](docs/architecture/memory.md), and [IPC Architecture Reference](docs/architecture/ipc.md).

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
