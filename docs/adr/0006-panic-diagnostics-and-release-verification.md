# ADR 0006: Panic Diagnostics and Release Verification

**Status:** Accepted  
**Date:** 2026-07-17  
**Deciders:** Gaxera project

## Context

At the Phase 5 baseline, the panic handler reports the Rust panic location and
message, then terminates. That proves the handler runs, but it is not enough to
diagnose the state of a freestanding kernel after a failed boot or safety
assertion. The v0.1 roadmap requires readable serial panic diagnostics,
including a stack trace and register dump, while preserving deterministic QEMU
verification.

The kernel intentionally discards `.eh_frame`, has no allocator guarantee on a
panic path, and has no symbolizer or debug-info reader in the runtime image.
General unwinding, DWARF parsing, allocation, and stack scanning would expand
the failure path substantially and cannot be made trustworthy in this release.

## Decision

Gaxera forces frame pointers for every `x86_64-unknown-none` kernel build with
`-C force-frame-pointers=yes`. The panic path emits only allocation-free serial
diagnostics:

1. the existing panic message and source location;
2. an architectural snapshot of `RSP`, `RBP`, `RFLAGS`, `CR2`, and the active
   `CR3` root frame;
3. at most 16 raw return instruction addresses from a frame-pointer chain;
4. a terminal reason and frame count for the bounded walk.

The walker reads a frame only when its aligned frame pointer and both words
reside wholly within the static bootstrap stack or the static double-fault IST
stack. Each link must increase monotonically because x86-64 stacks grow down.
It stops on an invalid link, a chain end, or the fixed frame limit. It never
scans arbitrary memory and never dereferences a pointer outside an owned stack.

Raw instruction addresses are intentionally not symbolized. They remain useful
with the corresponding ELF/map data, while claiming names without a robust
runtime symbol table would create misleading failure telemetry.

`xtask` records all required panic markers independently and accepts the
feature-gated panic QEMU profile only after it sees the panic location, CPU
state, a frame record, backtrace termination, and diagnostic completion, plus
the guest's `isa-debug-exit` success code.

## Consequences

Panic telemetry is bounded, deterministic, and available before heap
initialization. It does not establish a general exception unwinder, source
symbolizer, physical-hardware crash logger, or post-mortem persistence format.
The register snapshot describes the panic diagnostic context rather than an
immutable instruction-exception frame; `CR2` can be stale for non-page-fault
panics. Future stack switching, SMP, or dynamically allocated kernel stacks
must extend the owned-stack registry before relying on this walker there.

## Alternatives Considered

**DWARF or `.eh_frame` unwinding:** rejected for v0.1 because the linked image
deliberately discards that metadata and a reliable parser would enlarge the
least reliable execution path.

**Unbounded RBP-chain traversal:** rejected because corrupted stack state could
turn a diagnostic into another fault or a memory disclosure.

**General register capture through an assembly exception stub:** rejected
because a Rust panic has no hardware exception frame. It would add an ABI
surface without improving the trustworthiness of panic telemetry.
