# v1.2.0 Working-Tree Closeout Evidence

Date: 2026-08-05
Revision: v1.2 implementation tree at closeout
Environment: Linux x86_64 WSL2, QEMU 10.2.1, pinned project Rust nightly,
Limine 12.4.2

This checkpoint records the implementation-closeout evidence for v1.2. The
raw verification logs are retained alongside the bounded architectural scope.

## Verification performed

- `cargo fmt --all --check`: passed.
- `cargo test --workspace --locked`: passed. The workspace output included
  25 kernel tests, 97 kernel-core tests, and the remaining crate and integration
  suites with zero failures.
- `cargo xtask test`: passed. The command completed the locked build checks,
  profile-specific Clippy checks, the complete QEMU matrix, and normal ISO
  restoration. The final output was `All verification checks passed
  successfully!`.
- `cargo xtask run --headless --test irq-notification`: passed.
- `cargo xtask run --headless --test virtio-rng`: passed five consecutive clean
  runs after the PCI INTx routing correction.
- `cargo xtask run --headless --test driver-crash-restart`: passed five
  consecutive clean runs. The child is a generated Ring-3 VirtIO RNG image,
  not a synthetic kernel-only object test.
- `git diff --check`: passed.

## v1.2 proof boundary

The verified device path is QEMU's legacy VirtIO PCI interrupt line routed
through the IOAPIC using PCI's level-triggered, active-low semantics. MSI/MSI-X,
physical-machine driver validation, AP execution, and IOMMU isolation remain
future initiatives and are not claimed by this checkpoint.

## Evidence files

- `full-verification.log`: raw output captured from the complete matrix run.
- `virtio-rng-repeated.log`: raw concatenated output from five clean repeated
  VirtIO driver runs.
- `driver-crash-restart-repeated.log`: raw concatenated output from five clean
  integrated restart runs.
- `host-tests.log`: raw output captured from the final workspace host-test run.
