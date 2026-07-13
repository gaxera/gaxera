# Execution Roadmap — Zero to v0.1 (with v0.5 Checkpoints)

> **Status:** Canonical | **Version:** 1.0 | **Last Updated:** 2026-07-12
> **Related:** [Roadmap](../roadmap/roadmap_v01.md), [Constitution](../governance/constitution.md)

**Purpose:** A dependency-driven map of the path to v0.1.
This document explains the architecture of the work, the research gates, and the checkpoints.

## 1. Project Preconditions

**Status:** [RESEARCH REQUIRED]
Phase 1 implementation must not begin until the project has resolved or verified:

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

**Dependencies:** Preconditions Met.
**Objective:** Establish a reproducible, pinned build environment and continuous integration.

- **UEFI-Only Focus:** The architecture commits to UEFI. The build pipeline must produce a UEFI-compatible ISO. Omit legacy BIOS boot instructions.
- **xtask Pattern:** Implement a Rust-based build runner (`cargo xtask`) to manage building the kernel, creating the ISO, and launching QEMU.

#### 🚩 Checkpoint 1: Skeleton Kernel Built

- **Evidence:**
  - *CI Evidence:* A captured CI run artifact showing a successful `cargo build` of the skeleton kernel.
  - *Human Evidence:* First functional code merged into `main`.

### Phase 2: Limine Handoff & Boot to Screen

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

### Phase 3: CPU Exceptions & Interrupt Architecture

**Dependencies:** Phase 2.
**Objective:** Prove controlled exception handling and implement a foundational hardware interrupt (timer).

**Architecture & Subproblems:**

- **GDT/TSS:** Set up the Global Descriptor Table and Task State Segment. The TSS is necessary to provide a known-good stack for double faults.
- **Exception Handling:** Configure the Interrupt Descriptor Table (IDT) with handlers for Page Fault, General Protection Fault, and Double Fault.
- **PIC vs APIC:**
  - **🛑 RESEARCH GATE 3A:** Evaluate the 8259 PIC vs the APIC. Determine the migration implications for multi-core (SMP) support later. Decide on the v0.1 timer source (PIT vs LAPIC timer). Output: ADR on interrupt controller strategy.
- **Keyboard Input vs Timer:** The timer interrupt is sufficient to prove interrupt delivery, masking, and acknowledgement for v0.1. PS/2 keyboard input introduces additional driver overhead not strictly necessary for the foundational boot proof. Keyboard input is deferred to a post-v0.1 stretch goal or early v0.5 milestone.

**Validation Strategy for Unsafe Code:**
Do not rely solely on `cargo miri test` for bare-metal validation. Miri cannot emulate hardware registers or inline assembly.

- Use unit tests for architecture-independent logic.
- Use QEMU integration tests with deliberate fault injection.
- Conduct strict code reviews of unsafe invariants.

#### 🚩 Checkpoint 3: Interrupts Working

- **Evidence:**
  - *Machine Evidence:* A QEMU serial log proving a deliberate exception was caught by the handler, and log entries demonstrating successful timer interrupt acknowledgement.

### Phase 4: Physical & Virtual Memory Management

**Dependencies:** Phase 3.
**Objective:** Parse the memory map, manage physical frames, and establish virtual memory mapping.

**Architecture & Subproblems:**

- **Memory Map Interpretation:** Parse the Limine memory map to identify usable RAM.
- **Frame Allocator Design:**
  - **🛑 RESEARCH GATE 4A:** Evaluate bitmap vs linked-list frame allocators. Consider performance and initialization complexity. Output: ADR on frame allocator design.
- **Virtual Memory Model:** Understand x86_64 4-level paging.
- **Page-Table API:** Build abstractions to map, unmap, and translate virtual addresses to physical frames.
- **Heap Bootstrap:** Map a dedicated virtual region for the kernel heap and initialize a basic allocator to enable the `alloc` crate.
- **Allocator Testing:** Write host-testable unit tests for the allocator logic.

#### 🚩 Checkpoint 4: Memory Working

- **Evidence:**
  - *Machine/CI Evidence:* Deterministic test output from QEMU integration tests showing successful dynamic allocation (`Box`, `Vec`) and correct page translation.

### Phase 5: Polish & v0.1 Release

**Dependencies:** Phase 4.
**Objective:** Stabilize the kernel, ensure automated testing, and release v0.1.

**Architecture & Subproblems:**

- **QEMU Test Automation:** Use `isa-debug-exit` to allow the kernel to exit QEMU with a success code for CI integration.
- **Failure Diagnosis:** Ensure panics output a readable stack trace and register dump to the serial port.

#### 🚩 Checkpoint 5: v0.1 MILESTONE

- **Evidence:**
  - *CI Evidence:* A green CI artifact demonstrating a clean build and automated QEMU boot test pass.
  - *Human Evidence:* A tagged v0.1 release in the repository.

## 5. v0.5 Horizon (Dependency Graph)

After v0.1, the architecture progresses to basic OS functionality. This is a dependency map, not a scheduled plan.

- **Processes & Scheduler:** Context switching -> Preemptive timer -> EEVDF scheduler implementation.
- **IPC & Capabilities:** Endpoints -> Synchronous IPC -> Capability tokens.
- **User-Space & Services:** Syscall interface -> Init process -> Service Directory.
- **Basic Input & Shell:** PS/2 Keyboard Input -> In-memory filesystem -> basic command execution (`echo`, `cat`).
