# v1.2 Working-Tree Verification Summary

This checkpoint records actual verification from the uncommitted working tree,
not a release revision. The base revision is recorded in `source_revision.txt`.

## Passed

- `cargo fmt --all --check`
- strict Clippy for `xtask`, `kernel`, and all `userspace-tests` binaries
- `cargo test --workspace --locked`
- three consecutive `process-delegated-memory` QEMU runs
- complete `cargo xtask test` matrix, including the real `virtio-rng`, IRQ,
  process, allocator, exception, and crash/restart profiles

## Correctness fix represented by this checkpoint

`YieldProcess` was implemented in the userspace library through the generic
`sys_invoke` ABI, but the kernel dispatcher did not route that operation before
capability resolution. The dispatcher now handles `YieldProcess` directly and
returns the canonical status. This removed the intermittent delegated-memory
wait failure without weakening the test or adding a timing bypass.

The real VirtIO RNG profile also exposed a timing-sensitive DMA publication
issue. The guest now uses a release fence after publishing the descriptor and
available-ring entry, and an acquire fence before consuming the used-ring
completion. Three isolated VirtIO runs and the final full matrix passed after
that correction. The previous stage-marker diagnostics were removed.

## Explicit boundary

The real VirtIO RNG notification path and the generic fresh-process
crash/restart path are independently verified. The repository still lacks one
integrated test that supervises that same real VirtIO driver through crash and
restart while proving fresh device capabilities. v1.2 must not be tagged as
complete until that gate is implemented and verified.
