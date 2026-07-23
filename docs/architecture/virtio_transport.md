# VirtIO Transport & Device Subsystems Architecture (`virtio_transport`)

> **Status:** Current  
> **Date:** 2026-07-23  
> **Initiative:** Initiative v0.9 (Milestones 0.9.2, 0.9.3, 0.9.5)  
> **Applies To:** `crates/libgaxera::virtio`, `crates/virtio_block_server`, `crates/virtio_net_server`  

---

## 1. Executive Summary & Scope

The VirtIO Transport Subsystem provides allocation-free Virtqueue descriptor table management, available/used ring state machines, notification address calculations, and isolated ring-3 storage and network driver servers.

### Key Architectural Invariants
1. **Strict Protocol Boundary:** `virtio_block_server` and `virtio_net_server` are the protocol boundaries between generic `gaxera_abi` IPC interfaces (`block`, `net`) and VirtIO hardware device protocols. Client applications never observe VirtIO descriptors or hardware registers.
2. **Single-Owner DMA Lifetimes:** All transactions are encapsulated in RAII structures (`BlockRequest`, `PacketBuffer`) that automatically reclaim DMA memory on drop or connection loss.
3. **Encapsulated Notification MMIO Calculation:** Doorbell notification MMIO addresses are calculated by transport:
   $$\text{notify\_vaddr} = \text{notify\_base} + (\text{queue\_notify\_off} \times \text{notify\_off\_multiplier})$$

---

## 2. Component Design

### 2.1 Modern VirtIO 1.0 Transport (`libgaxera::virtio`)
- `Virtqueue`: Descriptor table (`VirtioDesc`), Available Ring (`VirtioAvailHeader`), Used Ring (`VirtioUsedElem`).
- Transport-owned size negotiation (`negotiate_queue_size`) verifying power-of-two constraints.
- Memory ordering fences (`Ordering::Release` for descriptor submission, `Ordering::SeqCst` for doorbell notifications).

### 2.2 VirtIO Block Storage Server (`virtio_block_server`)
- Formats 3-part descriptor chains: `VirtioBlkHeader` (`VRING_DESC_F_NEXT`), Data payload (`VRING_DESC_F_NEXT` or `VRING_DESC_F_WRITE`), and Status byte (`VRING_DESC_F_WRITE`).
- Translates raw VirtIO status bytes (`VIRTIO_BLK_S_OK`) into standardized `BlockError` IPC codes.

### 2.3 VirtIO Network Server (`virtio_net_server`)
- Formats 2-part descriptor chains with `VirtioNetHeader` (12 bytes).
- Manages dual queues: `RX_VQ` (pre-populated) and `TX_VQ`.
- Exposes `PacketBuffer` RAII state machine (`Free`, `RxReady`, `RxFilled`, `TxPending`, `Delivered`).
- Provides event-driven WaitSet IPC notifications and automatic client crash buffer reclamation.

---

## 3. Verification

- QEMU integration test `test-virtio-foundation` verifies queue size negotiation and notification MMIO address calculation.
- QEMU integration test `test-virtio-block` verifies sector read/write transactions.
- QEMU integration test `test-virtio-net` verifies MAC address caching and packet lifecycle states.
