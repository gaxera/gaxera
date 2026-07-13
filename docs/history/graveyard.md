# The Graveyard

> ⚠️ **HISTORICAL RECORD**
> This document is preserved from the project's design phase. It may receive editorial improvements for clarity, but its historical meaning and intent must remain unchanged.
> For current architecture, see [Technical Specification](../spec/technical_spec.md).

*Ideas we deliberately rejected, why we rejected them, and what we learned. Dead ideas still teach.*

---

## GRV-001: *(Removed)*

*Monolithic kernel was listed here based on Session 03 recommendations, but was never formally discussed or rejected. Moved back to the open decisions as an option.*

---

## GRV-002: "Just Build a Linux Distro"

**Proposed:** Session 01 (listed as option)
**Rejected:** Session 01

**What it was:** Instead of building a new OS, customize Linux with a unique desktop environment and tools.

**Why we rejected it:** Bee explicitly stated: "It won't be just another Linux distro." The project's ambition is to rethink computing from first principles, not to reskin an existing system.

**What we learned:** The temptation to "just use Linux" will come back. It always does. When it does, re-read the Constitution.

---

## GRV-003: AI as Product

**Proposed:** Implicitly, in early Vision A discussions
**Rejected:** Session 04

**What it was:** Making AI the visible, central feature of the OS — a chatbot interface, AI-generated content, AI making decisions.

**Why we rejected it:** Bee's principle: "AI is infrastructure, not destination." The OS should amplify human creativity, not automate humans away. AI that's visible and chatty captures attention rather than serving intention.

**What we learned:** The tech industry's obsession with making AI visible is a design failure. The best infrastructure is invisible.

---

## GRV-004: Vision H (Programmable OS) — As a Core Vision

**Proposed:** Session 02
**Status:** Not explicitly rejected, but not included in any phase

**What happened:** When presented with Visions G through L, Bee did not select H for inclusion. It wasn't rejected with reasoning — it simply didn't resonate.

**Note:** This may be revisited. The idea of making the OS scriptable/hackable could resurface during architecture design.

---

## GRV-005: Exokernel Architecture (Approach A)

**Proposed:** Session 07
**Rejected:** Session 07

**What it was:** Kernel provides zero abstractions — only secure hardware multiplexing. Each application brings its own "library OS" with whatever abstractions it wants.

**Why we rejected it:** Destroys system-wide AI (Vision A), system-wide knowledge graph (Vision B), and system-wide persistence (Vision K). If every app brings its own abstraction, there's no shared intelligence, no shared knowledge. The OS becomes a hardware multiplexer, not an intelligent system.

**What we learned:** Maximum flexibility at the kernel level can destroy the possibility of system-wide services. Some shared infrastructure MUST exist above the kernel.

---

## GRV-006: Reflective Kernel Architecture (Approach D)

**Proposed:** Session 07
**Rejected:** Session 07

**What it was:** A self-modifying kernel that can load new primitives at runtime, inspect its own internals, and hot-reload components.

**Why we rejected it:** Directly violates three constitutional principles: #5 (Security — can't verify what keeps changing), #6 (Immutability — self-modification is the opposite), #7 (Privacy — moving target can't guarantee boundaries).

**What we learned:** Maximum flexibility inside the kernel is incompatible with our security and immutability commitments. Flexibility must exist ABOVE the kernel, not within it.

---

## GRV-007: BIOS / Legacy Boot

**Proposed:** Legacy hardware support considerations.
**Rejected:** Architectural Sprint (Session 09)

**What it was:** Supporting legacy BIOS boot sequences alongside UEFI.
**Why we rejected it:** Supporting 16-bit real mode and legacy boot mechanisms adds massive complexity to the bootloader and kernel entry points, diluting the focus of v0.1. We are building for the future; UEFI has been the standard for over a decade.
**What we learned:** Ruthlessly cutting legacy support is required to maintain a small, auditable Trusted Computing Base.

---

## GRV-008: AI-Only Security Model

**Proposed:** Architectural Sprint (Session 08)
**Rejected:** Architectural Sprint (Session 08)

**What it was:** Relying exclusively on an AI "overseer" to monitor behavior and block malicious actions, replacing traditional access controls.
**Why we rejected it:** AI is non-deterministic and can be tricked (prompt injection, adversarial inputs). Security must be a structural, deterministic guarantee.
**What we learned:** AI behavioral detection is a fantastic *additional* layer (the "Immune System"), but the baseline security must be a rigid, deterministic capability system.

---

## GRV-009: Wayland (As a Default Unmodified Assumption)

**Proposed:** Linux Compatibility Subsystem Design
**Rejected:** Architectural Sprint (Session 08)

**What it was:** Adopting Wayland wholesale as our display protocol simply because it is the modern Linux standard.
**Why we rejected it:** Wayland's security boundaries and object models do not necessarily map cleanly onto our strict, explicit capability-based IPC. Adopting it blindly would compromise the core architecture.
**What we learned:** Never adopt a massive legacy standard just because it's popular. We must either build a custom native protocol or carefully adapt Wayland concepts to be capability-aware (now tracked in Research Debt).

---

*More entries will be added as we explore and eliminate ideas.*
