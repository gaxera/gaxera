# ADR-0044: Interrupt Delivery and Notification Semantics

**Status:** Accepted
**Date:** 2026-08-03
**Amends:** None

## Context

To enable real hardware drivers in Ring 3, physical device interrupts must be securely routed to userspace processes without compromising kernel stability. If the kernel parses complex device structures or performs allocations inside an Interrupt Service Routine (ISR), it risks unbounded latency, deadlocks, and kernel panics. If the kernel delegates vector ownership loosely, arbitrary processes can intercept or mask unrelated hardware events. We require a strictly scoped capability mechanism to bind vectors and an allocation-free ISR path to signal events.

## Decision

We introduce an end-to-end interrupt delivery pipeline using `InterruptObject` and `Notification` capabilities.
1. **Rights**: A new capability right, `Rights::INTERRUPT`, is introduced. Generic `Rights::WRITE` is insufficient to bind, mask, unmask, or acknowledge an interrupt.
2. **Capability Scoping**: A driver receives an explicit `InterruptObject` via its `BootstrapManifest`. The driver uses `InterruptControl` to bind this object to a specific `Notification`.
3. **ISR Semantics**: The kernel ISR performs zero dynamic allocation, acquires no long-held global locks, and parses no device protocols. It only performs a fast vector lookup, sends the LAPIC EOI, optionally masks the IOAPIC line (for level-triggered interrupts), and atomically sets the signal bits on the bound `Notification`.
4. **Driver Rearming (ACK)**: The driver uses `WaitNotification` to block. Upon wakeup, the driver drains the device, and then must explicitly call `InterruptControl(Ack)` to unmask the line and clear the binding state in the kernel.
5. **Teardown & Spurious Vectors**: When a driver exits, the interrupt is automatically masked and unbound, and the vector generation is incremented. Stale vector handles fail safely. Unbound/spurious vectors trigger a safe no-op EOI path.
6. **Controller Support**: v1.2.0 explicitly supports legacy level-triggered IOAPIC interrupts, verified end-to-end with the QEMU VirtIO RNG path. MSI/MSI-X are unsupported and will return typed errors until explicitly implemented and verified.

## Consequences

- **Easier**: Driver development is fully isolated to Ring 3; a driver crash safely masks the interrupt and tears down the vector without kernel disruption.
- **Easier**: Verification is simplified because the ISR boundary is extremely minimal.
- **Harder**: Drivers cannot use synchronous polling as their primary event loop. They must correctly implement the mask/ACK state machine, otherwise the hardware line will remain masked indefinitely.

## Alternatives Considered

- **In-Kernel Device Drivers**: Moving performance-critical drivers (like VirtIO Net) into the kernel to avoid the syscall overhead of interrupt delivery. Rejected as it fundamentally violates Gaxera's microkernel philosophy and crash containment boundaries.
- **Counting Semaphores for Interrupts**: Allowing an unbounded event counter to increment on every ISR. Rejected because it can theoretically overflow if the driver is stalled. Using a coalesced bitmask in `Notification` guarantees bounded state.
