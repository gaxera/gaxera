# VirtIO Reference Platform Master Specification

> **Status:** Canonical | **Milestone Target:** v0.9.5 | **Version:** 1.0  
> **Related Documents:** [ADR 0028](../adr/0028-driver-framework-and-dma-capability-infrastructure.md), [ADR 0029](../adr/0029-virtio-foundation-and-virtqueue-transport-architecture.md), [ADR 0036](../adr/0036-virtio-gpu-input-reference-platform.md)

---

## 1. Overview & Architecture

The **Gaxera VirtIO Reference Platform** provides a 100% complete, capability-secured virtual hardware foundation on QEMU:

```
+---------------------------------------------------------------------------------------+
|                    Gaxera VirtIO Reference Platform (Ring-3 User Space)               |
+---------------------------------------------------------------------------------------+
|                                                                                       |
|  [virtio_block_server]  ──► Mass Storage (CoW Extents & Superblocks)                    |
|  [virtio_net_server]    ──► Network Frames (PacketRing Zero-Copy Handoff)             |
|  [virtio_gpu_server]    ──► 2D/3D Display Scanout & Framebuffer Management             |
|  [virtio_input_server]  ──► Keyboard & Mouse Event Capability Routing                     |
|                                                                                       |
+---------------------------------------------------------------------------------------+
        ▲                            ▲                            ▲
        | Microkernel Capabilities   | DMA ContiguousFrame        | MSI-X IRQ Objects
        v                            v                            v
+---------------------------------------------------------------------------------------+
|                    Gaxera Microkernel Core (Ring-0 Mechanism Only)                    |
+---------------------------------------------------------------------------------------+
```

---

## 2. VirtIO-GPU Display Architecture (`virtio_gpu_server`)

### 2.1 Queue Specifications
- `controlq` (Queue 0): 256-entry Virtqueue for 2D/3D GPU command submission.
- `cursorq` (Queue 1): 256-entry Virtqueue for hardware cursor position updates.

### 2.2 2D Display Scanout Protocol Sequence
1. **`VIRTIO_GPU_CMD_RESOURCE_CREATE_2D`:** Creates 2D texture resource (width, height, `VIRTIO_GPU_FORMAT_B8G8R8A8_UNORM`).
2. **`VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING`:** Attaches DMA `ContiguousFrame` physical memory pages to resource.
3. **`VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D`:** Transfers modified pixel rects to host GPU memory.
4. **`VIRTIO_GPU_CMD_SET_SCANOUT`:** Binds 2D resource to display scanout head (Scanout 0).
5. **`VIRTIO_GPU_CMD_RESOURCE_FLUSH`:** Triggers host screen refresh.

---

## 3. VirtIO-Input Event Architecture (`virtio_input_server`)

### 3.1 Queue Specifications
- `eventq` (Queue 0): 256-entry Virtqueue for receiving raw hardware input events.
- `statusq` (Queue 1): 256-entry Virtqueue for sending status updates (LEDs, force feedback).

### 3.2 Standardized Input Event Types
```rust
#[repr(C)]
pub struct VirtioInputEvent {
    pub event_type: u16, // EV_KEY (1), EV_REL (2), EV_ABS (3)
    pub code: u16,       // Key code or Axis identifier
    pub value: u32,      // Key state (0=up, 1=down, 2=repeat) or Axis value
}

#[derive(Copy, Clone, Debug)]
pub enum InputEvent {
    Keyboard { key_code: u16, is_pressed: bool },
    PointerMove { dx: i32, dy: i32 },
    PointerButton { button: u8, is_pressed: bool },
}
```

### 3.3 Focus Capability Scoping
- Keystrokes and pointer events are dispatched exclusively to processes holding an active **`FocusHandle`** capability derived from `init` supervisor. Unprivileged background services cannot intercept keystrokes.
