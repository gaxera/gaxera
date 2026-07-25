# ADR 0036: VirtIO-GPU & VirtIO-Input Reference Platform Architecture

* **Status:** Accepted
* **Date:** 2026-07-25
* **Target Release:** v0.9.5
* **Author:** Gaxera DeepMind Architecture Team
* **Deciders:** Kernel Core, Driver Framework, Graphics & Input Subsystem Leads

---

## 1. Context and Problem Statement

Following the completion of storage (`virtio_block_server`) and networking (`virtio_net_server`), Gaxera requires hardware-accelerated display rendering and user input event handling to complete the **100% VirtIO Reference Platform on QEMU**.

Kernel Ring 0 must remain mechanism-only, containing **zero graphics drivers, display compositors, or input loops**. Display scanout and input event routing must run entirely in unprivileged Ring-3 user space while preserving capability security and zero-copy shared memory performance.

---

## 2. Decision Outcome

We decision to implement **Milestone 0.9.5** via two decoupled Ring-3 driver servers:

1. **`virtio_gpu_server` (Ring-3 User-Space Display Driver):**
   - Maps VirtIO-GPU PCI Express BAR via microkernel `Mapping` capabilities.
   - Allocates 256-entry Virtqueues (`controlq` for GPU commands, `cursorq` for hardware cursor movement).
   - Implements VirtIO-GPU 2D Display Pipeline:
     * Resource Creation (`VIRTIO_GPU_CMD_RESOURCE_CREATE_2D`)
     * Resource Backing Memory Attachment (`VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING`)
     * Host Transfer (`VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D`)
     * Scanout Display Binding & Flush (`VIRTIO_GPU_CMD_SET_SCANOUT`, `VIRTIO_GPU_CMD_RESOURCE_FLUSH`)
   - Shared-memory zero-copy framebuffer allocation for future window compositors.

2. **`virtio_input_server` (Ring-3 User-Space Input Driver):**
   - Maps VirtIO-Input PCI Express BAR via microkernel `Mapping` capabilities.
   - Manages VirtIO Input Virtqueues (`eventq` for hardware input events, `statusq` for LED/feedback).
   - Decodes Linux `input_event` format (`EV_KEY`, `EV_REL`, `EV_ABS`) into standardized Gaxera input handles (`KeyEvent`, `PointerEvent`).
   - Capability-gated input event streams: apps only receive input events if they hold an active focus capability handle, structurally preventing system-wide keylogging.

---

## 3. Consequence & Invariants

- **Zero Ring-0 Graphics Code:** Microkernel kernel-core remains 100% free of display pipeline or input handling code.
- **Zero-Copy Frame Buffer Transfer:** Framebuffers are mapped directly to physical memory pages and passed to `virtio_gpu_server` without payload copies.
- **Keylogger Prevention:** Input streams are scoped by capability focus handles; unprivileged background processes cannot capture ambient keystrokes.
- **100% VirtIO Reference Platform:** Completes full QEMU VirtIO reference suite (Block, Net, GPU, Input).
