# Gaxera Documentation

This directory contains the public documentation, architectural specifications, and design history of the Gaxera project.

## How to Read This

### If you want to understand what Gaxera is

1. [Constitution](governance/constitution.md) — Core principles
2. [Technical Specification](spec/technical_spec.md) — What we're building
3. [Foundation v0.1 Architecture Reference](architecture/foundation_v0.1.md) —
   The released kernel baseline and its enforced boundaries
4. [v0.5 Requirements Trace](architecture/v05_requirements_trace.md) —
   The frozen release scope against long-term requirements

### If you want to understand why we made specific choices

1. [Kernel Candidate Evaluation](history/kernel_candidate_evaluation.md) — Why from scratch
2. [Graveyard](history/graveyard.md) — Ideas we rejected and why
3. [ADRs](adr/) — Formal architecture decisions

### If you want to understand the design journey

1. [Sessions 01–07](history/sessions/) — Design exploration conversations
2. [Chronicle](history/chronicle.md) — Project timeline

### If you want to build or contribute

1. [Roadmap](roadmap/roadmap_v01.md) — Milestone structure
2. [v0.5 Engineering Program](roadmap/roadmap_v05.md) — Frozen next-release
   architecture and implementation program
3. [Developer Workflow](development/workflow.md) — Build, run, and verification commands
4. [Environment](environment.md) — Tested host and emulator boundary
5. [Evidence](evidence/) — Checkpoint proof and provenance
6. [CONTRIBUTING.md](../CONTRIBUTING.md) — Contribution workflow

## Document Taxonomy

| Classification | Meaning | Can be edited? | Examples |
| --- | --- | --- | --- |
| **Canonical** | Source of truth for the project's architecture and plans. Changes require discussion or an ADR. | Yes, through ADR process | Technical Spec, Roadmap |
| **Historical** | Records from the project's design phase. Preserved for context and lineage. | Editorial improvements only — meaning must remain unchanged | Sessions, Chronicle, Graveyard |
| **Governance** | Operating procedures, principles, and session records. | Yes, by maintainer | Constitution, Build Log |
| **ADR** | Architecture Decision Record. Immutable once accepted. Superseded by newer ADRs, never edited. | No — supersede instead | ADR-0000, ADR-0001, etc. |
| **Evidence** | Checkpoint proof (logs, screenshots, CI artifacts). Committed and never modified. | No — append only | Serial logs, QEMU screenshots |
