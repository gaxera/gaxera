# Phase 4 Tag Provenance Addendum

The implementation-verification record
`2026-07-16_phase4_commit-ff0c724_verification.log` accurately records a
successful exact-commit run for `ff0c7245ece63956b7b773c6b255f40c56833de8`.
Its final sentence reflected the intended tag target at the time it was
written.

Before final closeout, the Phase 4 engineering handoff was moved from the
repository root to the ignored `.internal/` directory at the maintainer's
request. That documentation-only change was committed as
`f7a8f53686b894620a803157d1dd3f1c0b742645` and the complete `cargo xtask test`
matrix was rerun successfully on that exact revision.

The annotated `phase-4-complete` tag therefore points to `f7a8f53686b894620a803157d1dd3f1c0b742645`, as proven by
`2026-07-16_phase4_commit-f7a8f53_verification.log`. This addendum supersedes
only the earlier record's tag-target statement; it does not alter the earlier
verification result.

Both `.log` files are concise, committed verification records rather than raw
terminal transcripts. They enumerate the exact command, target commit,
environment, profiles, markers, and exit result. The full transient QEMU serial
output remains in ignored local `logs/qemu-*.log` files; it is intentionally not
published as repository evidence.
