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

🚧 **v0.1 — early development.**

* **Phase 1 (Toolchain & Scaffolding):** Completed.
* **Phase 2 (Boot Contract & Observability):** Completed.
* **Phase 3 (Robust Exceptions):** Active planning.

Detailed phase milestones and progress maps are tracked in the [v0.1 Roadmap](docs/roadmap/roadmap_v01.md).

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
