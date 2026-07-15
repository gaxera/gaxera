# Evidence Log

| Date | Checkpoint | Artifact | Target commit | Environment | Bootloader | Verification |
| --- | --- | --- | --- | --- | --- | --- |
| 2026-07-15 | Phase 2 | `checkpoint-02/2026-07-15_phase2_boot_verification.log` | `7bfd67f042e12d74ca6defeb2f096d04762a003a` plus uncommitted Phase 1/2 work | Ubuntu 26.04 WSL2; QEMU 10.2.1; Rust nightly-2026-07-13 | Limine 12.4.2 | `cargo xtask test` passed BIOS and UEFI handoff with all three serial markers, then verified a feature-gated UEFI panic reports its source location and message. |
| 2026-07-15 | Phase 2 | `checkpoint-02/2026-07-15_phase2_framebuffer.png` | `7bfd67f042e12d74ca6defeb2f096d04762a003a` plus uncommitted Phase 1/2 work | Ubuntu 26.04 WSL2; QEMU 10.2.1; Rust nightly-2026-07-13 | Limine 12.4.2 | QEMU monitor framebuffer capture after the UEFI gradient proof. |
