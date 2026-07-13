# ADR-0000: Engineering Philosophy

**Status:** Accepted
**Date:** 2026-07-11

## Context

Gaxera requires a formal mechanism for recording and governing architectural
decisions. Without this, decisions drift, rationale is lost, and the project's
architectural integrity degrades over time.

## Decision

All architectural decisions are recorded as Architecture Decision Records (ADRs)
in `docs/adr/`. The following rules govern the ADR process:

1. **What requires an ADR:** Any decision that changes or constrains the
   project's architecture, security model, tooling strategy, dependency set,
   or governance. Routine implementation choices do not.

2. **Immutability:** Accepted ADRs are never edited. If a decision is
   reversed, a new ADR supersedes the old one. The original is preserved.

3. **Source-of-truth hierarchy:**
   Constitution → Technical Spec → ADRs → Roadmap → Session docs.
   Lower-level documents cannot silently override higher-level ones.

4. **Amending the TRD:** The Technical Specification (`technical_spec.md`)
   can only be amended through an accepted ADR that explicitly identifies
   the section being changed.

5. **Committed architectural constraints** (e.g., items explicitly marked `[COMMITTED]`) in the Technical Spec require an ADR to change. Open exploration items (e.g., `[RESEARCH REQUIRED]`) can be resolved by an ADR or by implementation evidence.

## Consequences

- Every significant decision has a traceable, permanent record.
- Future contributors can understand *why*
  something was decided, not just *what* was decided.
- The process adds a small amount of overhead per decision, which is
  justified by the reduction in architectural drift.
