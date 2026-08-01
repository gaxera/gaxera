# Hardware Interrupt Delivery & Driver Notification Pipeline Architecture

> **Status:** Draft — deferred from the v1.1.0 Ring-3 memory foundation
> **Initiative:** v1.1.0 — Device Event & Ring-3 Runtime Foundation  
> **Scope:** Hardware IRQs, IOAPIC/MSI Routing, InterruptObject Capabilities, Notification Signaling  
> **Related ADRs:** ADR 0005, ADR 0013, ADR 0022  

---

## 1. Program Charter

### 1.1 Problem Statement
In Gaxera `v1.0.0`, the microkernel manages Local APIC timers and exception vectors, but unprivileged Ring-3 driver servers (`virtio_net_server`, `virtio_input_server`, `virtio_gpu_server`, `virtio_block_server`) cannot yet receive hardware device interrupts end-to-end. Drivers currently rely on synchronous polling or simulated events.

To enable real device I/O in a future interrupt initiative, Gaxera requires
an end-to-end interrupt delivery pipeline that maps hardware IRQ vectors to
kernel `InterruptObject` capabilities and signals Ring-3 driver `Notification`
objects without admitting driver policy into Ring 0. This document is
architecture research only; no v1.1.0 release claim or passing hardware-IRQ
implementation is based on it.

### 1.2 Non-Goals
* **Kernel Driver Execution:** Device drivers remain 100% in Ring 3. Zero driver policy or packet/event decoding logic is admitted into kernel ISRs.
* **Heavyweight Kernel ISR Processing:** Kernel ISRs do not allocate memory, acquire heavy locks, block, or parse device register structures.
* **Ambient IRQ Binding:** Ring-3 processes cannot bind or acknowledge arbitrary interrupt vectors without possessing a valid `InterruptObject` capability handle with `Rights::INTERRUPT`.

### 1.3 Key Questions Answered by This Architecture
1. How does a hardware device interrupt transition from physical IRQ vector to a Ring-3 `Notification` signal?
2. What are the exact mask, unmask, acknowledge (ACK), and rearm semantics for level-triggered vs. edge-triggered interrupts?
3. How are spurious vectors, interrupt affinity, and CPU migration handled?
4. What happens when a Ring-3 driver crashes or exits while an interrupt binding is pending?
5. How is RG-1 verification decoupled from RG-2 (Ring-3 Allocator) for independent release?

### 1.4 Affected Interfaces
* `crates/gaxera-abi`: `InterruptOp` (`BindNotification = 1`, `Mask = 2`, `Unmask = 3`, `Ack = 4`), `OperationCode::InterruptControl`.
* `kernel/src/arch/x86_64`: `idt.rs`, `apic.rs`, `ioapic.rs`, `msi.rs`, `interrupt.rs`.
* `crates/libgaxera`: `syscall::interrupt_control`, `object::InterruptObject`.

### 1.5 Evidence Criteria for Completion
* **Host Unit Tests:** Interrupt vector registry lookup, notification binding state machine, mask/unmask/ack state transitions, and teardown cleanup.
* **QEMU Integration Tests:**
  1. Future `test-irq-notification`: QEMU VirtIO device interrupt (e.g. `virtio-net` or keyboard IRQ) triggering LAPIC vector $\rightarrow$ kernel ISR $\rightarrow$ `Notification` signal $\rightarrow$ Ring-3 driver `WaitNotification` wakeup.
  2. `test-unauthorized-irq-rejection`: Process without `InterruptObject` capability denied access to interrupt control operations.
  3. `test-driver-crash-irq-teardown`: Driver exit automatically unbinding vector, masking IOAPIC/MSI line, and clearing vector ownership without kernel panic or vector leakage.

---

## 2. End-to-End Interrupt Delivery Pipeline

```
+-------------------+
| Hardware Device   | (PCIe MSI-X / Legacy IOAPIC Line)
+---------+---------+
          | Physical Interrupt Signal
          v
+-------------------+
| LAPIC Interrupt   | (Vector 32..255)
+---------+---------+
          | CPU Trap Execution
          v
+-------------------+
| Kernel Minimal ISR| (kernel/src/arch/x86_64/idt.rs)
| - Send LAPIC EOI  | (No allocation, no heavy locking)
| - Mask Line (Level)|
+---------+---------+
          | Fast Vector Lookup
          v
+-------------------+
| InterruptObject   | (Bound to target Notification ID)
+---------+---------+
          | Signal Bitmask
          v
+-------------------+
| Notification      | (Atomic bitwise OR of signal bits)
+---------+---------+
          | Scheduler Wakeup
          v
+-------------------+
| Ring-3 Driver     | (Wakes from WaitNotification / WaitSet)
| Process (CPL 3)   | (Executes driver I/O, then issues Ack/Unmask)
+-------------------+
```

---

## 3. Interrupt Semantics and Lifecycle

### 3.1 Triggering Modes and Rearm Semantics
* **Level-Triggered Interrupts (Legacy IOAPIC):**
  1. Kernel ISR sends LAPIC EOI and masks the IOAPIC line immediately to prevent interrupt storms.
  2. Signals the driver's `Notification` object.
  3. Ring-3 driver drains device hardware queues, then calls `InterruptControl(Ack)` or `InterruptControl(Unmask)`.
  4. Kernel unmasks the IOAPIC line.
* **Edge-Triggered Interrupts (MSI / MSI-X):**
  1. Kernel ISR sends LAPIC EOI (no line masking required).
  2. Signals the driver's `Notification` object.
  3. Ring-3 driver wakes, processes events, and calls `InterruptControl(Ack)` if rearming is required.

### 3.2 Spurious Vectors and Vector Allocation
* Vectors 0..31 are reserved for processor exceptions.
* Vectors 32..255 are managed by the kernel `VectorRegistry`.
* Spurious interrupts (e.g. vector 255) are handled by a no-op handler returning EOI without signaling notifications.

### 3.3 Driver Exit and Vector Teardown
When a Ring-3 driver process crashes or exits:
1. `CapabilitySpace` teardown drops all `InterruptObject` handles owned by the process.
2. If reference count reaches 0:
   * Kernel unbinds the `InterruptObject` from the `Notification`.
   * Masks the physical IOAPIC line / disables the MSI-X vector.
   * Returns the vector back to `VectorRegistry`.
3. Ensures zero vector leaks or dangling interrupt handlers.

---

## 4. Security & Capability Invariants

1. **INV-IRQ-01 (Capability Scoping):** Ring-3 processes cannot bind, mask, unmask, or acknowledge interrupts without a valid `InterruptObject` capability handle carrying `Rights::INTERRUPT`.
2. **INV-IRQ-02 (Bounded ISR Execution):** Kernel ISRs execute in $< 1\text{ }\mu\text{s}$, perform zero dynamic memory allocations (`SlabCache` or frame allocator), and acquire zero long-held spinlocks.
3. **INV-IRQ-03 (Fault Isolation):** Driver crashes or unhandled interrupts cannot disable global CPU interrupts (`cli`) or crash the kernel.

---

## 5. Independent Releasability (RG-1 vs. RG-2)

RG-1 (IRQ Delivery) and RG-2 (Ring-3 Allocator) proceed through completely independent architecture, implementation, and verification gates:
* RG-1 can be verified on QEMU using static Ring-3 memory buffers before RG-2 dynamic allocation is integrated.
* RG-1 will receive a separate implementation and release decision after its
  architecture and bootstrap dependencies are complete; it is not merged into
  the v1.1.0 memory-foundation baseline.
