# Gaxera Documentation

This directory contains the public documentation, architectural specifications, and design history of the Gaxera project.

## How to Read This

### If you want to understand what Gaxera is

1. [Constitution](governance/constitution.md) — Core principles
2. [Technical Specification](spec/technical_spec.md) — What we're building

### If you want to understand why we made specific choices

1. [Kernel Candidate Evaluation](history/kernel_candidate_evaluation.md) — Why from scratch
2. [Graveyard](history/graveyard.md) — Ideas we rejected and why
3. [ADRs](adr/) — Formal architecture decisions

### If you want to understand the design journey

1. [Sessions 01–07](history/sessions/) — Design exploration conversations
2. [Chronicle](history/chronicle.md) — Project timeline

### If you want to build or contribute

1. [Roadmap](roadmap/roadmap_v01.md) — Milestone structure
2. [CONTRIBUTING.md](../CONTRIBUTING.md) — Contribution workflow

## Document Taxonomy

| Classification | Meaning | Can be edited? | Examples |
| --- | --- | --- | --- |
| **Canonical** | Source of truth for the project's architecture and plans. Changes require discussion or an ADR. | Yes, through ADR process | Technical Spec, Roadmap |
| **Historical** | Records from the project's design phase. Preserved for context and lineage. | Editorial improvements only — meaning must remain unchanged | Sessions, Chronicle, Graveyard |
| **Governance** | Operating procedures, principles, and session records. | Yes, by maintainer | Constitution, Build Log |
| **ADR** | Architecture Decision Record. Immutable once accepted. Superseded by newer ADRs, never edited. | No — supersede instead | ADR-0000, ADR-0001, etc. |
| **Evidence** | Checkpoint proof (logs, screenshots, CI artifacts). Committed and never modified. | No — append only | Serial logs, QEMU screenshots |
