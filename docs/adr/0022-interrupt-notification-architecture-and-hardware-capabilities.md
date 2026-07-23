# ADR 0022: Interrupt & Notification Architecture & Hardware Capability Model

> **Status:** Accepted  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.8.1 — Interrupt & Notification Architecture (`docs/roadmap/roadmap_v08.md`)  
> **Applies To:** `gaxera-abi`, `kernel-core`, `kernel`  

---

## Context & Problem Statement

In Gaxera's capability microkernel architecture, user-mode device drivers (such as keyboard drivers, storage drivers, and network protocol engines) execute in unprivileged ring-3 address spaces. To service physical hardware without running ring-0 code or possessing ambient root authority, drivers require:

1. A capability-mediated mechanism to receive hardware interrupt signals (IRQs).
2. A capability model for managing physical MMIO ranges (`ObjectType::Mapping`).
3. Integration with `WaitSet` event loops so driver threads can wait on hardware IRQs, timer deadlines, and client IPC messages within a single unified event loop.

---

## Decision

We adopt **Capability-Bound Hardware Interrupt Delivery via `InterruptObject` & `Notification`**:

1. **First-Class `InterruptObject` Kernel Capability (`ObjectType::InterruptObject = 7`):**
   - Represents exclusive, unforgeable capability authority over a specific hardware IRQ line (e.g. IRQ 1 for PS/2 keyboard, IRQ 16 for NVMe PCI vector).
   - Provides explicit single-responsibility operations via `InterruptOp`:
     - `BindNotification`: Bind an `InterruptObject` capability to a target `Notification`.
     - `Mask`: Explicitly mask the IOAPIC redirection line.
     - `Unmask`: Explicitly unmask the IOAPIC redirection line.
     - `Ack`: Issue LAPIC/IOAPIC hardware EOI acknowledgement without modifying the mask state.

2. **Lightweight Pure `Notification` Kernel Object (`ObjectType::Notification = 4`):**
   - **Pure Signal State Machine (ADR 0013 Compliant):** `Notification` holds only `signals: u32` bitfield state and metadata. It maintains **zero** subscriber queues or waiter thread references (`Vec<ObjectId>`), keeping `Notification` objects 100% fixed-size with zero dynamic memory allocation.
   - **Kernel-Signaled Primitive:** Notifications bound to IRQs are signaled exclusively by kernel ISR execution. `SignalNotification` is omitted from the public syscall ABI to preserve capability security.
   - **`WaitSet` Multiplexing Integration:** `WaitSet` continues owning all waiter thread scheduling and event subscription management (`MAX_WAITSET_SUBSCRIPTIONS = 64`, `MAX_WAITSET_EVENTS = 128`). When an IRQ fires, the kernel signals the `Notification` and notifies any bound `WaitSet`.

3. **Hardware Interrupt Delivery Pipeline:**
   - **IOAPIC & LAPIC Vectors:** Kernel allocates hardware IDT vector slots (`32..255`) and programs IOAPIC Redirection Table entries.
   - **ISR Execution:** When an IRQ fires, the kernel ISR masks the IOAPIC entry, sends LAPIC EOI, signals the bound `Notification`, and posts a readiness event to subscribers.
   - **User-Space Event Loop:** The driver thread receives the notification via `WaitSetWait`, processes device MMIO, calls `Ack` to acknowledge IRQ, and calls `Unmask` when ready for the next IRQ.

---

## Rationale & Alternatives Considered

### Alternative 1: Dynamic `Vec<ObjectId>` inside Notification — REJECTED
* **Pros:** Allows notifications to maintain their own waiter thread lists.
* **Cons:** Conflates scheduler/waiter management into notification objects (violating ADR 0013) and risks unbounded kernel heap allocations.

### Alternative 2: Implicit Unmasking in `AckInterrupt` — REJECTED
* **Pros:** Combines acknowledgement and unmasking into a single syscall.
* **Cons:** Conflates single responsibilities; risks re-entrancy race conditions if driver MMIO processing is incomplete when EOI is issued.

---

## Consequences & Invariants

1. **Strict Subsystem Isolation:** `Notification` is a pure signal state machine; `WaitSet` and `syscall.rs` own all thread waiting and scheduler interactions.
2. **Bounded Memory Invariant:** `Notification` objects have a fixed byte footprint with zero dynamic heap allocations.
3. **Single-Responsibility Syscalls:** Masking, unmasking, acknowledgement, and notification binding are decoupled into explicit operations.
