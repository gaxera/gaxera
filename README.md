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

**v0.1 — Foundation release.** Tagged `v0.1.0` and `phase-6-complete` at
`f6b2146`; validated by the deterministic UEFI QEMU matrix.

**v0.5 — Capabilities & Microkernel Program.** Active development (tagged `v0.5-m3-complete`).

* **M0 (Setup & Baseline Preservation):** Completed.
* **M1 (Object Arena & Capability Model):** Completed in `kernel-core` with host-tested derivation and revocation state machines.
* **M2A (Privilege Transition & Isolated Address Space):** Completed and verified under UEFI QEMU (DPL-3 GDT/TSS configuration, isolated user page tables, internal ring-3 return gate).
* **M2B (Syscall ABI & Fault-Recoverable User Access):** Completed and verified under UEFI QEMU (`syscall`/`sysret` MSR setup, `CpuLocal` GS base, and fault-recoverable `copy_from_user` / `copy_to_user` routines).
* **M3 (Threads & Cooperative Execution):** Completed and verified under UEFI QEMU. Gaxera now possesses a proven System V ABI-compliant context switch, a generic capability-integrated thread ownership model, and deterministic state-enforced scheduling.

Detailed v0.1 and v0.5 milestones and progress maps are tracked in [v0.1 Roadmap](docs/roadmap/roadmap_v01.md) and [v0.5 Roadmap](docs/roadmap/roadmap_v05.md).
The exact released architecture and proposed program are documented
in the [Foundation v0.1 Reference](docs/architecture/foundation_v0.1.md) and
[v0.5 Engineering Program](docs/roadmap/roadmap_v05.md).

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
