# Contributing to Gaxera

Thank you for your interest in Gaxera.

## Current Status

This is currently a solo project in early development. Contributions are
welcome, but please open an issue to discuss before submitting large changes.

## Development Setup

See [docs/roadmap/roadmap_v01.md](docs/roadmap/roadmap_v01.md)
for the high-level phases and [docs/development/workflow.md](docs/development/workflow.md)
for public environment setup, build, and verification instructions.

## Workflow

1. Read [docs/README.md](docs/README.md) for project context.
2. Branch from `main`, PR back in.
3. Use [Conventional Commits](https://www.conventionalcommits.org/) for
   commit messages.
4. All checks must pass before merge:
   - `cargo fmt --all -- --check`
   - `cargo clippy --locked -- -D warnings`
   - `cargo xtask test`
   - CI green

## Code Standards

- **No `unsafe` without a `// SAFETY:` comment.** You must be able to
  explain the invariant the code relies on and what happens if it is violated.
- **No new dependencies without discussion.** Open an issue first.
- **Architectural changes require an ADR.** See
  [docs/adr/TEMPLATE.md](docs/adr/TEMPLATE.md).

## Decision Process

Decisions flow through the ADR (Architecture Decision Record) process, not
informal agreement. See [docs/governance/constitution.md](docs/governance/constitution.md)
for core principles.
