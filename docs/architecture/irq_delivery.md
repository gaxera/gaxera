# Hardware Interrupt Delivery & Driver Notification Pipeline Architecture

> **Status:** Current / legacy IOAPIC mechanism verified
> **Initiative:** v1.2.0 — Device Event and Ring-3 Runtime Completion
> **Scope:** Hardware IRQs, IOAPIC Routing, InterruptObject Capabilities, Notification Signaling

## 1. Program Charter

This document defines the end-to-end hardware interrupt delivery pipeline in Gaxera v1.2. It maps physical IRQ vectors to kernel `InterruptObject` capabilities and signals Ring-3 driver `Notification` objects.

Crucially, driver policy remains entirely in Ring 3. The kernel ISR only performs minimal bookkeeping (ACK/masking) and notification signaling.

## 2. Supported and Unsupported Controllers
**Implemented mechanism in the current tree:**
- Legacy IOAPIC vector allocation, generation checking, masking, binding,
  teardown, and capability authorization.

**Verified in the current tree:**
- The `test-virtio-rng` QEMU profile attaches QEMU's legacy VirtIO RNG device,
  discovers it through PCI/MCFG, maps its capability-backed MMIO windows and
  DMA frame, binds an `InterruptObject`, waits in Ring 3, observes the device
  completion interrupt, acknowledges/rearms it, and confirms completion.
- The independent `test-irq-notification` profile verifies two delivery cycles
  and deterministic runner synchronization.

**Unsupported / Future Work:**
- MSI and MSI-X are explicitly unsupported in this release. The types or stubs may exist for future work, but no documentation may claim they are complete without independent verification evidence. Attempts to bind unsupported modes return a typed error.

## 3. End-to-End Interrupt Delivery Pipeline

```
+-------------------+
| Hardware Device   | (Legacy IOAPIC Line)
+---------+---------+
          | Physical Interrupt Signal
          v
+-------------------+
| LAPIC Interrupt   | (Vector 32..255)
+---------+---------+
          | CPU Trap Execution
          v
+-------------------+
| Kernel Minimal ISR|
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
| Ring-3 Driver     | (Wakes from WaitNotification)
| Process (CPL 3)   | (Executes driver I/O, then issues Ack/Unmask)
+-------------------+
```

## 4. Interrupt Semantics and Lifecycle

### 4.1 Triggering Modes and Rearm Semantics
For level-triggered IOAPIC interrupts (the v1.2 implemented path):
1. The kernel ISR immediately masks the IOAPIC line to prevent interrupt storms, sends a LAPIC EOI, and signals the driver's `Notification` object.
2. The normal kernel context drains pending notification wakeups and transitions blocked driver Threads to Runnable.
3. The Ring-3 driver wakes up, drains the device hardware queues, and processes the event.
4. The Ring-3 driver must explicitly call `InterruptControl(Ack)` to rearm the interrupt.
5. The kernel validates the driver's authority, unmasks the IOAPIC line, and clears the binding state.

### 4.2 Spurious Vectors and Vector Allocation
- Vectors 0..31 are reserved for processor exceptions.
- Vectors 32..255 are managed by the kernel `VectorRegistry`.
- Vector allocation is bounded and fallible.
- A released vector receives a new generation count. A late physical interrupt arriving on a stale generation cannot signal a newly bound driver.
- Spurious interrupts (e.g. vector 255) are handled by a no-op handler returning EOI without signaling notifications.

### 4.3 Driver Exit and Vector Teardown
When a Ring-3 driver process crashes or exits:
1. `CapabilitySpace` teardown drops all `InterruptObject` handles.
2. The interrupt is automatically masked and unbound from the driver's notification.
3. The vector generation is incremented, and the vector is returned to the `VectorRegistry`.
4. Double-unbind and stale-generation operations fail safely.

## 5. Security & Capability Invariants

1. **INV-IRQ-01 (Capability Scoping):** Ring-3 processes cannot bind, mask, unmask, or acknowledge interrupts without a valid `InterruptObject` capability handle explicitly carrying `Rights::INTERRUPT`. Generic `Rights::WRITE` is insufficient.
2. **INV-IRQ-02 (Bounded ISR Execution):** Kernel ISRs perform zero dynamic memory allocations (`SlabCache` or frame allocator), acquire zero long-held spinlocks, parse no device protocols, and perform only bounded vector lookup.
3. **INV-IRQ-03 (Fault Isolation):** Driver crashes or unhandled interrupts cannot crash the kernel. The driver is cleanly torn down, and the interrupt is safely masked.

## 6. Verification Requirements
- `test-irq-notification`: verifies two real notification deliveries and
  deterministic ACK/rearm behavior.
- `test-virtio-rng`: verifies the real device-generated VirtIO-to-Ring-3 path.
- `test-irq-unauthorized`: A process without the `InterruptObject` capability is denied access.
- `test-irq-driver-teardown`: Driver exit automatically unbinds the vector, masks the IOAPIC line, and clears vector ownership without kernel panic or vector leakage.
- `test-irq-vector-reuse`: Validates that vector generations protect against stale interrupt bindings.
