# ADR 0029: VirtIO Foundation & Virtqueue Transport Architecture

> **Status:** Proposed  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.9.2 — VirtIO Foundation (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `crates/libgaxera`, `crates/pci_bus_server`  

---

## Context & Problem Statement

Gaxera requires a generic, high-performance VirtIO transport abstraction for ring-3 microkernel drivers (`virtio_block_server`, `virtio_net_server`).

VirtIO 1.0 (Modern PCI transport) requires:
1. Parsing VirtIO Vendor-Specific Capabilities (`0x09`) in PCI configuration space to discover location and BAR offsets for:
   - Common Configuration Structure (`VIRTIO_PCI_CAP_COMMON_CFG = 1`)
   - Notifications Structure (`VIRTIO_PCI_CAP_NOTIFY_CFG = 2`)
   - ISR Status Structure (`VIRTIO_PCI_CAP_ISR_CFG = 3`)
   - Device-Specific Configuration Structure (`VIRTIO_PCI_CAP_DEVICE_CFG = 4`)
2. Virtqueue size negotiation and DMA memory allocation (`ContiguousFrameHandle`).
3. Modern notification address calculation and doorbell writes.

---

## Transport Architectural Invariants

> **Invariant 1 — Strict Transport Boundary:**  
> The `libgaxera::virtio` module is strictly a VirtIO 1.0 PCI transport layer. It owns PCI capability discovery, feature handshake, Virtqueue negotiation, ring memory management, fence ordering, notification address calculation, and ISR register clearing. It contains zero block-, network-, or device-specific protocol logic.
> 
> **Invariant 2 — Encapsulated Addressing & Negotiation:**  
> Drivers never calculate notification MMIO addresses or execute queue size negotiation independently. All queue sizing and MMIO doorbell offset math are executed internally by `VirtioPciTransport`.

---

## Technical Decisions

### 1. Transport-Owned Virtqueue Size Negotiation Algorithm
- `VirtioPciTransport::setup_queue(q_idx, req_size)` owns 100% of queue size negotiation:
  1. Write `queue_select = q_idx`.
  2. Read hardware maximum queue size `max_size` from `queue_size` register. Validate `max_size > 0`.
  3. Compute `negotiated_size = min(req_size, max_size)`. Verify `negotiated_size` is a non-zero power of two.
  4. Write `negotiated_size` back to hardware `queue_size` register.
  5. Allocate DMA-backed `ContiguousFrameHandle` for `negotiated_size` descriptors, avail ring, and used ring.

### 2. Encapsulated Notification MMIO Address Calculation
- `VirtioPciTransport` computes notification MMIO virtual addresses internally per queue during queue setup:
  $$\text{notify\_vaddr} = \text{notify\_base} + (\text{queue\_notify\_off} \times \text{notify\_off\_multiplier})$$
  - `notify_base`: MMIO base virtual address of the Notify Capability structure.
  - `notify_off_multiplier`: 32-bit multiplier read from `VIRTIO_PCI_CAP_NOTIFY_CFG` header in PCI config space.
  - `queue_notify_off`: 16-bit offset read from `queue_notify_off` register after selecting `q_idx`.
- **Doorbell Action:** When `notify_queue(q_idx)` is called, `VirtioPciTransport` issues a `fence(Ordering::SeqCst)` and writes `q_idx` directly to `notify_vaddr`. Drivers never perform address arithmetic.

### 3. Feature Negotiation Protocol & Strict Validation
- Handshake sequence:
  1. Reset device (`status = 0`).
  2. Set `ACKNOWLEDGE` (0x01) and `DRIVER` (0x02) status bits.
  3. Read `device_features`. Compute `negotiated = device_features & driver_offered`.
  4. Verify mandatory features (must include `VIRTIO_F_VERSION_1`). If missing, abort with `VirtioError::MissingMandatoryFeature`.
  5. Write `driver_features` and set `FEATURES_OK` (0x08) status bit.
  6. Re-read status register. If hardware clears `FEATURES_OK`, device rejected features $\rightarrow$ Reset device (`status = 0`) and fail immediately with `VirtioError::FeatureNegotiationFailed`.

### 4. Virtqueue Lifecycle Ownership (`VirtioPciTransport`)
- `VirtioPciTransport` executes full 9-step Virtqueue setup:
  1. Write `queue_select = q_idx`.
  2. Negotiate queue size.
  3. Compute notification MMIO address.
  4. Allocate DMA `ContiguousFrameHandle`.
  5. Resolve physical base address via `sys_invoke(frame, QueryPhysicalAddress)`.
  6. Write `queue_desc`, `queue_driver`, `queue_device` 64-bit physical addresses.
  7. Write `queue_enable = 1`.
  8. Repeat for all queues.
  9. Set `DRIVER_OK` (0x04) status bit.

### 5. Encapsulated Memory Ordering & Barrier Semantics
- `libgaxera::virtio` owns all memory barriers internally:
  - **Descriptor Submission:** Release fence (`fence(Ordering::Release)`) before updating Available Ring index.
  - **Doorbell Notification:** Full memory fence (`fence(Ordering::SeqCst)`) before MMIO doorbell write.
  - **Used Ring Consumption:** Acquire fence (`fence(Ordering::Acquire)`) before reading Used Ring descriptors.

### 6. Single-Owner Linear DMA Ownership Model
- `Virtqueue` takes **owned move semantics** of `ContiguousFrameHandle`.
- Zero sharing or aliases. Unmaps memory and releases handle on drop.

### 7. Standardized Transport Error Model (`VirtioError`)
```rust
pub enum VirtioError {
    MissingCapability(u8),
    UnsupportedVersion,
    FeatureNegotiationFailed,
    MissingMandatoryFeature,
    InvalidQueueSize(u16),
    DmaAllocationFailed,
    QueueEnableFailed,
    DeviceResetFailed,
}
```

---

## Consequences & Invariants

1. **Zero Notification Math in Drivers:** Drivers call `notify_queue(q_idx)`; all MMIO offset multiplier arithmetic is encapsulated in transport.
2. **Standardized Sizing:** Queue size negotiation is 100% unified across all VirtIO drivers.
3. **Driver Simplicity:** VirtIO Block and VirtIO Network drivers submit requests and consume completions without implementing PCI transport details.
