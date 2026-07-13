# Kernel Candidate Evaluation — Build vs. Fork vs. Borrow

> ⚠️ **HISTORICAL RECORD**
> This document is preserved from the project's design phase. It may receive editorial improvements for clarity, but its historical meaning and intent must remain unchanged.
> For current architecture, see [Technical Specification](../spec/technical_spec.md).

We are evaluating four primary paths against the 14 requirements extracted in `kernel_requirements.md`:

1. **seL4:** The mathematically verified, capability-based microkernel.
2. **Redox OS Kernel:** The modern Rust-based microkernel.
3. **Zircon (Fuchsia):** Google's modern capability-based microkernel.
4. **Custom Build:** Building it from scratch in Rust as currently planned in the TRD.

---

## 1. seL4

**The Pitch:** The most secure operating system kernel in the world. It is the only kernel with a mathematical proof of functional correctness and isolation. It uses exactly the capability model we want.

| Requirement | Score | Notes |
| --- | --- | --- |
| **Architecture** | 🟩 Pass | Perfect microkernel. Absolutely minimal. |
| **Language** | 🟥 Fail | Written in C (and mathematically modeled in Haskell/Isabelle). Not Rust. |
| **Capability System** | 🟩 Pass | The gold standard. Exactly what we modeled our TRD after. |
| **IPC Model** | 🟩 Pass | Lightning fast, supports capability passing. |
| **Scheduler** | 🟨 Partial | Solid, but modifying it for our 5-class model is brutally hard due to proofs. |
| **IOMMU** | 🟩 Pass | Excellent hardware isolation. |
| **Size/Complexity** | 🟥 Fail | Code size is small (~10K LOC), but the *cognitive complexity* is massive. |
| **License** | 🟩 Pass | GPLv2 for kernel (acceptable for standalone binary), BSD for userland. |

**Verdict:** seL4 gives us the exact security and capability model we want, but it forces us to write C (or build complex Rust bindings) and makes modifying the kernel near-impossible without breaking the mathematical proofs. It is incredibly hostile to fast iteration.

---

## 2. Redox OS Kernel

**The Pitch:** A modern, Unix-like microkernel written entirely in Rust. It's the closest existing project to our language and safety goals.

| Requirement | Score | Notes |
| --- | --- | --- |
| **Architecture** | 🟨 Partial | Microkernel, but with a lot of Unix baggage (URLs for everything). |
| **Language** | 🟩 Pass | 100% Rust. |
| **Capability System** | 🟥 Fail | Uses a URL-based scheme scheme (`scheme://path`) and traditional Unix-like user/group permissions, not pure unforgeable capability tokens. |
| **IPC Model** | 🟨 Partial | Fast, but semantics are tied to their scheme design. |
| **Scheduler** | 🟨 Partial | Good, but not tailored for our real-time/deadline requirements. |
| **IOMMU** | 🟨 Partial | Work in progress. |
| **Size/Complexity** | 🟩 Pass | Very clean, readable Rust codebase. |
| **License** | 🟩 Pass | MIT License. |

**Verdict:** Redox is a fantastic project, but it is deeply committed to being a "Unix-like" Rust OS. To get our pure capability system and drop the Unix baggage, we would have to rip out the core of its IPC and naming systems. We'd spend more time fighting its design than utilizing it.

---

## 3. Zircon (Fuchsia)

**The Pitch:** Google's modern microkernel designed for mobile/desktop. It uses a pure object-based capability system and handles handles exactly like we want.

| Requirement | Score | Notes |
| --- | --- | --- |
| **Architecture** | 🟩 Pass | Excellent microkernel design. |
| **Language** | 🟥 Fail | Written in C++. |
| **Capability System** | 🟩 Pass | Exactly the handle/rights model we want. |
| **IPC Model** | 🟩 Pass | Channels, sockets, VMOs (shared memory). |
| **Scheduler** | 🟩 Pass | Very modern deadline scheduling included. |
| **IOMMU** | 🟩 Pass | Yes. |
| **Size/Complexity** | 🟥 Fail | Massive codebase, heavily tied to Google's bespoke build system (GN/Ninja). |
| **License** | 🟩 Pass | MIT/BSD-style. |

**Verdict:** Architecturally, Zircon is almost a perfect match for our TRD. But the C++ codebase and the Google-specific build tooling make it a nightmare for a small indie team to fork, maintain, and integrate with a pure Rust user-space ecosystem.

---

## 4. Custom Build (From Scratch in Rust)

**The Pitch:** Build exactly the 10 kernel objects and ~50 syscalls we need in Rust, as specified in the TRD. No legacy code, no C++, no Unix baggage.

| Requirement | Score | Notes |
| --- | --- | --- |
| **Architecture** | 🟩 Pass | We build exactly what we need. |
| **Language** | 🟩 Pass | 100% Rust. |
| **Capability System** | 🟩 Pass | Native to the design. |
| **IPC Model** | 🟩 Pass | Tailored for our exact performance targets. |
| **Scheduler** | 🟩 Pass | We implement the exact 5-class model. |
| **IOMMU** | 🟩 Pass | Built-in from day one. |
| **Size/Complexity** | 🟨 Partial | We control the size, but the initial complexity of getting SMP, ACPI, and PCIe working is daunting. |
| **License** | 🟩 Pass | MIT/Apache 2.0 |

**Verdict:** It requires solving the "boring" problems (page tables, APIC, ACPI) ourselves. But it guarantees that our foundation perfectly matches our philosophy.

---

## Conclusion

> **Note:** This evaluation reflects the initial design phase analysis and is preserved for historical context. The project has since committed to a custom build.

If our goal was *just* to build a secure system quickly, we would fork seL4.
If our goal was *just* to use Rust quickly, we would fork Redox.

But our goal is to build an **intent-driven, capability-secure OS without Unix baggage**.

- Forking seL4 traps us in C and prevents rapid architectural iteration.
- Forking Redox forces us to inherit Unix design patterns.
- Forking Zircon traps us in Google's C++ build ecosystem.

**The hard truth:** There is no existing microkernel that combines 100% Rust, pure capability-based security, and zero Unix legacy.

To achieve the vision, **we must build the kernel from scratch.**

However, we shouldn't do it blindly. We should aggressively *plagiarize the architecture* of Zircon and seL4 while writing the implementation in Rust, leveraging the `x86_64` crate ecosystem in Rust to skip the boilerplate hardware initialization.
