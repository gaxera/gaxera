# Execution Roadmap — Zero to v0.1 (with v0.5 Checkpoints)

> **Status:** Canonical | **Version:** 1.0 | **Last Updated:** 2026-07-17
> **Related:** [Roadmap](../roadmap/roadmap_v01.md), [Constitution](../governance/constitution.md)

**Purpose:** A dependency-driven map of the path to v0.1.
This document explains the architecture of the work, the research gates, and the checkpoints.

## 1. Project Preconditions

**Status:** Complete for the implemented v0.1 bootstrap.
Phase 1 began only after the project resolved or verified:

1. Rust nightly/toolchain version and target strategy.
2. QEMU version and invocation behavior.
3. Limine version/branch/protocol API.
4. ISO/image construction tooling.
5. Host dependencies, Git/GitHub CLI workflow, and CI environment compatibility.

## 2. Repository Bootstrap & Workflow

**Policy:**

1. Bootstrap commits may be pushed directly to `main` to establish the foundational structure and CI pipeline.
2. Once Checkpoint 1 (Repository Ready) is achieved, strict branch protection MUST be enabled on `main`.
3. All subsequent work follows feature branches and pull requests.

**Repository Structure:**
Create directories only when the component exists or an immediate implementation step requires it. Avoid speculative empty scaffolding. *(Note: Directories are created strictly on-demand. If a component is not yet implemented, its folder does not exist in the repo).*

```text
gaxera/
├── .github/workflows/          # CI/CD pipelines
├── docs/                       # Architecture, decisions, and research
├── kernel/                     # Kernel crate
│   ├── src/
│   │   ├── arch/x86_64/        # Architecture-specific code
│   │   └── main.rs
│   └── Cargo.toml
├── xtask/                      # Build automation
└── Cargo.toml                  # Workspace root

```

## 3. Evidence Categories & AI Collaboration

### Checkpoint Evidence Categories

- **Machine Evidence:** Deterministic output that can be parsed or validated automatically (e.g., serial markers, test exit codes).
- **Human Evidence:** Artifacts useful for visual inspection and project history (e.g., framebuffer screenshots, boot recordings).
- **CI Evidence:** Evidence proving reproducibility in automation (e.g., successful clean build, automated QEMU timeout handling).

### AI-Assisted Code

**Rule for Code Acceptance:** AI-generated code is NEVER accepted merely because it compiles. The project owner must be able to explain the relevant invariant, ownership boundary, hardware assumption, and failure mode before merging safety-critical or unsafe code.

### Decision Escalation Workflow

When implementation reveals that an existing design decision may be wrong:

1. **STOP** implementation of the affected path.
2. Write down the exact conflict.
3. Identify the original decision source (e.g., ADRs, Technical Specification, or Constitution).
4. Research alternatives (prototype or benchmark if necessary).
5. Create or update an Architecture Decision Record (ADR).
6. Update the Technical Reference Document (`technical_spec.md`).
7. Resume implementation.

## 4. Execution Phases

### Phase 1: Toolchain & Repository Bootstrap

**Status:** Complete.
**Dependencies:** Preconditions Met.
**Objective:** Establish a reproducible, pinned build environment and continuous integration.

- **UEFI-Only Focus:** The architecture commits to UEFI. The build pipeline must produce a UEFI-compatible ISO. Hybrid ISO packaging may retain BIOS data only as an optional diagnostic path; it is never a supported architecture or release target.
- **xtask Pattern:** Implement a Rust-based build runner (`cargo xtask`) to manage building the kernel, creating the ISO, and launching QEMU.

#### 🚩 Checkpoint 1: Skeleton Kernel Built

- **Evidence:**
  - *CI Evidence:* A captured CI run artifact showing a successful `cargo build` of the skeleton kernel.
  - *Human Evidence:* First functional code merged into `main`.

### Phase 2: Limine Handoff & Boot to Screen

**Status:** Complete.
**Dependencies:** Phase 1.
**Objective:** QEMU reaches the Rust kernel entry through Limine, proving the boot contract.

**Architecture & Subproblems:**

- **Freestanding Rust:** Understand `#![no_std]`, panic handlers, and the x86_64 calling convention.
- **Limine Protocol Integration:** Define the Limine request structs (framebuffer, memory map) to receive bootloader data.
- **Kernel Entry:** Ensure the `_start` symbol is correctly exported and invoked.
- **Serial Debugging:** Initialize the UART 16550 serial port. This provides critical observability before the framebuffer is fully usable.
- **Framebuffer Access:** Parse the Limine framebuffer response and write a test pattern.

#### 🛑 RESEARCH GATE 2A

- **Understand before coding:** ELF program headers, Limine request/response model, x86 I/O ports vs MMIO for serial.
- **Proceed when:** You can explain how Limine locates your kernel and what state the CPU is in when `_start` is called.

#### 🚩 Checkpoint 2: First Boot

- **Evidence:**
  - *Machine Evidence:* Captured serial boot log outputting a deterministic marker (e.g., `Gaxera: KERNEL_ENTRY_OK`).
  - *Human Evidence:* A QEMU screenshot showing the framebuffer test pattern.

### Phase 3: Robust Exceptions

**Status:** Complete.
**Dependencies:** Phase 2.
**Objective:** The kernel catches CPU exceptions cleanly and reaches a terminal Double Fault handler on a dedicated stack without triple-faulting.

**Architecture & Subproblems:**

- **GDT/TSS:** Set up the Global Descriptor Table and Task State Segment. The TSS is necessary to provide a known-good stack for double faults.
- **Exception Handling:** Configure the Interrupt Descriptor Table (IDT) with handlers for Page Fault, General Protection Fault, Division Error, Breakpoint, and Double Fault.
- **IST Stack:** Allocate a static stack in `.bss` for the double fault Interrupt Stack Table (IST).

**Validation Strategy for Unsafe Code:**
Do not rely solely on `cargo miri test` for bare-metal validation. Miri cannot emulate hardware registers or inline assembly.

- Use unit tests for architecture-independent logic.
- Use QEMU integration tests with deliberate fault injection.
- Conduct strict code reviews of unsafe invariants.

**Firmware policy:** UEFI is the required validation target. BIOS may remain an optional diagnostics path while the hybrid ISO exists, but it is not an architectural or release requirement.

#### 🚩 Checkpoint 3: Exceptions Working

- **Evidence:**
  - *Machine Evidence:* A UEFI QEMU serial log proving a deliberate breakpoint exception was caught by the handler, and log entries demonstrating processor-delivered Double Fault entry on the IST stack, including an RSP-in-IST-stack assertion (not a triple fault or `int $8`).

### Phase 4: Physical & Virtual Memory Foundations

**Status:** Complete. Research Gates 4A-4C are closed. The immutable boot
context, physical-frame allocator, owned four-level CR3 transition, RAM-only
HHDM, guarded heap, and deterministic memory/guard-page proofs are verified
under UEFI QEMU. The Phase 5 constraints are recorded in ADRs 0003 and 0004.
**Dependencies:** Phase 3.
**Objective:** Parse the memory map, manage physical frames, establish virtual memory mapping, and bootstrap a kernel heap.

**Architecture & Subproblems:**

- **Memory Map Interpretation:** Parse the Limine memory map to identify usable RAM.
- **Boot Context and Memory Map:** Capture Limine input into a Gaxera-owned immutable `BootContext` and emit a canonical classified memory-map diagnostic before allocation.
- **Frame Allocator Design:**
  - **🛑 RESEARCH GATE 4A:** Closed by ADR 0003. Use a bootstrap range allocator followed by a segmented bitmap allocator over validated usable memory.
- **Virtual Memory Model:** Understand x86_64 4-level paging.
- **Page-Table API:** Build abstractions to map, unmap, and translate virtual addresses to physical frames.
  - **🛑 RESEARCH GATE 4B:** Closed by ADR 0004. Request exact four-level paging; use `MappedPageTable`, a RAM-only HHDM, W^X/NX mapping policy, and a reviewed CR3 continuity transition.
- **Heap Bootstrap:** Map a dedicated virtual region for the kernel heap and initialize a basic allocator to enable the `alloc` crate.
  - **🛑 RESEARCH GATE 4C:** Closed by ADR 0004. Use a guarded fixed heap and an exact-pinned reviewed allocator; define allocation failure explicitly.
- **Allocator Testing:** Write host-testable unit tests for the allocator logic.

#### 🚩 Checkpoint 4: Memory Working

- **Evidence:**
  - *Machine/CI Evidence:* Deterministic UEFI QEMU output from `cargo xtask test`
    showing successful post-CR3 dynamic allocation (`Box`, `Vec`), correct
    first-heap-page translation, and a deliberate lower-heap-guard page fault
    whose CR2 value matches the unmapped guard address.

### Phase 5: ACPI Discovery & APIC Timer Proof

**Status:** Implementation in progress. Research Gate 5A is closed by
[ADR 0005](../adr/0005-acpi-local-apic-and-timer-delivery.md). The current
implementation and complete pre-commit UEFI matrix are complete; exact
commit/tag provenance remains part of Phase 5 closeout.
**Dependencies:** Phase 4.
**Objective:** Discover the BSP Local APIC via ACPI tables, map it with
verified Uncacheable attributes, and prove deterministic timer interrupt
delivery without introducing timer calibration or scheduler semantics.

**Architecture & Subproblems:**

- **ACPI Discovery:** Locate the RSDP (via Limine), walk RSDP -> XSDT -> MADT to extract the Local APIC base physical address.
- **MMIO Caching & Aliasing:**
  - **Research Gate 5A:** Closed by ADR 0005. Gaxera owns a minimal bounds-checked parser, a page-at-a-time temporary firmware mapping, a typed permanent UC xAPIC mapping, BSP-only xAPIC mode, and delivery-only timer semantics.
- **APIC Timer Setup:** Configure the APIC Timer in periodic mode and route it to vector `0xe0`; vector `0xff` handles spurious delivery.
- **Interrupt Handler:** Write a non-allocating timer handler that increments an atomic ticks counter, masks at the exact test target, and sends EOI.

#### 🚩 Checkpoint 5: Interrupts & Timer Working

- **Evidence:**
  - *Machine Evidence:* Serial log showing ACPI MADT parsing, release of the temporary window, Local APIC base address, and exactly three periodic timer deliveries.

### Phase 6: Stabilization & v0.1 Release

**Dependencies:** Phase 5.
**Objective:** Harden the kernel, ensure automated testing, and release v0.1.

**Architecture & Subproblems:**

- **QEMU Test Automation:** Preserve and broaden the existing `isa-debug-exit`-based QEMU verification as memory and timer proofs are added.
- **Failure Diagnosis:** Ensure panics output a readable stack trace and register dump to the serial port.

#### 🚩 Checkpoint 6: v0.1 MILESTONE

- **Evidence:**
  - *CI Evidence:* A green CI artifact demonstrating a clean build and automated UEFI QEMU boot test pass.
  - *Human Evidence:* A tagged v0.1 release in the repository.

## 5. v0.5 Horizon (Dependency Graph)

After v0.1, the architecture progresses to basic OS functionality. This is a dependency map, not a scheduled plan.

- **Processes & Scheduler:** Context switching -> Preemptive timer -> EEVDF scheduler implementation.
- **IPC & Capabilities:** Endpoints -> Synchronous IPC -> Capability tokens.
- **User-Space & Services:** Syscall interface -> Init process -> Service Directory.
- **Basic Input & Shell:** PS/2 Keyboard Input -> In-memory filesystem -> basic command execution (`echo`, `cat`).
