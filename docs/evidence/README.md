# Execution Evidence Directory

> **Status:** Governance | **Last Updated:** 2026-07-17

This directory serves as the immutable record of successful checkpoint executions. The Gaxera project demands proof for architectural claims. If an implementation phase claims to work, the deterministic evidence proving it belongs here.

## 1. Naming Convention

All evidence files MUST follow this naming convention:
`YYYY-MM-DD_phaseX_evidence_type.ext`

**Examples:**

- `YYYY-MM-DD_phase1_build_success.log`
- `YYYY-MM-DD_phase2_framebuffer.png`
- `YYYY-MM-DD_phase3_timer_interrupt.log`

## 2. Accepted Formats

To ensure the repository remains fast to clone and easy to search, only the following formats are permitted:

1. **Text Logs (`.log`, `.txt`):** For all serial output, QEMU traces, build artifacts, and crash dumps.
2. **Images (`.png`, `.jpg`, `.webp`):** Exclusively for human-visual proofs where text is insufficient (e.g., a colored rectangle on a framebuffer, a screenshot of a panic screen). *Do not upload BMPs or uncompressed images.*
3. **Markdown (`.md`):** For human-readable summaries or transcripts of test procedures.

*Video files, core dumps, and ISOs are strictly prohibited in the repository. Host large artifacts externally if required.*

## 3. Required Metadata

To maintain a deterministic and easily searchable record, all evidence files must be registered in an `EVIDENCE_LOG.md` file maintained in this directory. The log entry for each evidence file must define:

- **Target Commit:** The exact Git commit hash being tested.
- **Environment:** The host OS, QEMU version, and Rust nightly date.
- **Bootloader:** The Limine version used for the test.

## 4. Checkpoint Mapping

Evidence should map directly to the checkpoints defined in `roadmap_v01.md`:

| Phase | Expected Evidence |
| --- | --- |
| **Phase 1** | CI build output proving compilation on `x86_64-unknown-none`. |
| **Phase 2** | Serial log of kernel entry; Framebuffer screenshot. |
| **Phase 3** | UEFI serial log of caught Breakpoint and processor-delivered Double Fault, including an RSP-in-IST-stack assertion and guest-confirmed exit. |
| **Phase 4** | UEFI serial log of owned CR3, page translation, `Box`/`Vec` allocation, and a lower heap guard-page fault with exact CR2. |
| **Phase 5** | UEFI serial log of ACPI/MADT discovery, temporary firmware-window release, Local APIC setup, and exact timer-delivery ticks. |
| **Phase 6** | Full CI workflow artifact for v0.1 completion. |
