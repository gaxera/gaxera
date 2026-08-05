# Driver Lifecycle Architecture

> **Status:** Current / policy, process containment, and integrated real-driver
> restart verified in the QEMU reference environment
> **Initiative:** v1.2.0 — Device Event and Ring-3 Runtime Completion

## 1. Program Charter
This document defines the lifecycle, supervision, and crash containment of Ring-3 drivers in Gaxera. Drivers execute in unprivileged userspace, meaning their lifecycle events—discovery, capability installation, crash handling, and restarts—must be explicitly managed by a supervisor.

## 2. Driver Supervisor
The `DriverSupervisor` (which may be `init` or a dedicated userspace supervisor) is the intended policy owner. The current tree contains the policy object and an integrated Ring-3 supervisor test that builds a real VirtIO RNG driver image, provisions it through the bootstrap manifest, observes a device-generated notification, reaps the deliberately crashed process, and starts a fresh replacement with fresh device capabilities.

The supervisor's responsibilities include:
- Parsing driver executable images and allocating `MemoryObjects`.
- Creating the child Process and ResourceDomain.
- Explicitly granting MMIO capabilities.
- Explicitly granting InterruptObject and Notification capabilities.
- Starting the driver.
- Listening for the driver's exit Notification.
- Determining crash containment policies (e.g., restart limits, backoff).

## 3. Interrupts and Crash Containment
When a driver process crashes, the kernel forces it into the `ExitRequested` state and executes a deterministic teardown. During teardown, the kernel guarantees:
- All device capabilities held by the driver are deleted.
- If an interrupt was bound, it is unmasked (wait, no, it is MASKED) and unbound from the driver's notification.
- The vector generation is incremented, ensuring that any stale vector handles or late-arriving physical interrupts cannot signal a new driver incorrectly.
- The exit Notification is signaled to the supervisor.

## 4. Driver Restart and Revocation
To restart a crashed driver, the supervisor must create a completely fresh `Process`, `Thread`, `AddressSpace`, and `CapabilitySpace`. It cannot resurrect a dead thread.
- The new driver receives freshly created device capabilities and a new `InterruptObject`.
- Any handles to the old vector generation held elsewhere will fail safely.
- If a supervisor wishes to forcefully revoke a driver's access, it issues a `Terminate` on the process, triggering the same teardown and containment path.

## 5. Event Loop
Drivers must use `WaitNotification` to handle hardware events. Polling is not an acceptable primary event path for physical interrupt-driven drivers. Upon receiving a notification, the driver must ACK/rearm the interrupt according to the controller contract (e.g., IOAPIC level-triggered masking/unmasking rules).

## 6. Verification Requirements
- `test-driver-crash-restart`: verifies the integrated real-driver path: the
  generated child performs real VirtIO queue submission, waits for the device
  IRQ through a capability-scoped Notification, acknowledges/rearms the IRQ,
  exits deliberately, is reaped by the supervisor, and is replaced by a fresh
  process with fresh device capabilities.
- `test-irq-driver-teardown`: Validates that a driver exit safely unbinds and masks its interrupt.
- `test-irq-vector-reuse`: Validates that vector generations protect against stale interrupt bindings.
- `test-virtio-rng`: validates the standalone real VirtIO RNG device interrupt
  path and Ring-3 notification/ACK/rearm behavior. The integrated restart
  profile additionally exercises this path before crash and replacement.
