# ADR 0032: VirtIO Network Server & Zero-Copy Packet IPC Architecture (`virtio_net_server`)

> **Status:** Proposed  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.9.5 — VirtIO Network Server (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `crates/virtio_net_server`, `crates/libgaxera`, `crates/gaxera-abi`  

---

## Context & Problem Statement

Gaxera requires an unprivileged ring-3 network driver process (`virtio_net_server`) to provide high-throughput network packet transmission (TX) and reception (RX) for microkernel user applications.

VirtIO Network Specification (VirtIO 1.0) requires:
1. Receiving BAR `Mapping` and IRQ `Interrupt` capabilities delegated by `pci_bus_server`.
2. Initializing VirtIO Modern PCI transport via `libgaxera::virtio`.
3. Negotiating `VIRTIO_NET_F_MAC` (bit 5) and reading device MAC address (6 bytes) from `VIRTIO_PCI_CAP_DEVICE_CFG`.
4. Managing two Virtqueues: `RX_VQ` (Queue 0) and `TX_VQ` (Queue 1).
5. Pre-populating `RX_VQ` with zero-copy DMA packet buffers formatted with `VirtioNetHeader` (12 bytes).
6. Providing zero-copy shared memory packet IPC capabilities between `virtio_net_server` and client applications.

---

## Architectural Invariants

> **Invariant 1 — Layer-2 Protocol Boundary:**  
> `virtio_net_server` provides strictly Layer-2 Ethernet frame transport. ARP, IPv4, IPv6, ICMP, UDP, TCP, DHCP, DNS, routing, sockets, and higher-layer protocols run 100% in user space. Client applications never observe raw Virtqueue structures or hardware registers.
> 
> **Invariant 2 — Single-Owner Packet State Machine:**  
> A `PacketBuffer` is owned by exactly one entity and exists in strictly one state (`Free`, `RxReady`, `RxFilled`, `TxPending`, `Delivered`) at any instant.
> 
> **Invariant 3 — Automatic Client Crash Reclamation:**  
> Leased `PacketBuffer` instances are bound to the client connection capability. Client termination or handle destruction automatically triggers `virtio_net_server` buffer reclamation, returning all orphaned buffers to the `Free` pool with zero memory leaks.
> 
> **Invariant 4 — DMA Synchronization Safety:**  
> Software MUST NOT mutate or release a `PacketBuffer` payload while it is in `RxReady` or `TxPending` state (owned by NIC DMA). Hardware returns buffer ownership only after a Virtqueue Used Ring completion interrupt.

---

## Technical Decisions

### 1. `PacketBuffer` Struct & State Lifecycle
- Each packet buffer is encapsulated in a `PacketBuffer` struct:
  ```rust
  #[derive(Clone, Copy, Debug, Eq, PartialEq)]
  pub enum PacketState {
      Free,
      RxReady,
      RxFilled,
      TxPending,
      Delivered,
  }

  pub struct PacketBuffer {
      pub buffer_id: u16,
      pub header: VirtioNetHeader,
      pub dma_handle: ContiguousFrameHandle,
      pub payload_len: u16,
      pub state: PacketState,
  }
  ```

### 2. Zero-Copy Shared Memory Model & Automatic Reclamation
- `virtio_net_server` creates shared packet pools backed by derived `Mapping` capabilities (ADR 0020).
- **Client Crash Reclamation:** `virtio_net_server` tracks leased buffers per client handle. If a client terminates or closes its connection endpoint, `virtio_net_server` automatically reclaims all leased buffers back to `Free` state and replenishes `RX_VQ`.

### 3. Event-Driven WaitSet Notification Model
- Applications register their local `WaitSetObject` with `ENDPOINT_NET_SERVICE`.
- When an RX packet arrives, `virtio_net_server` signals the client's `NotificationObject`.
- The client wakes from `sys_waitset_wait`, executes non-blocking `NetOp::ReceivePacket`, reads the zero-copy frame payload from shared memory, and releases the buffer via `NetOp::ReleaseBuffer`.

### 4. RX Virtqueue Replenishment & Drop Policy
- `virtio_net_server` pre-populates `RX_VQ` with up to `QueueSize` (e.g. 64) buffers in `RxReady` state.
- **Exhaustion Behavior:** If all free packet buffers are leased to clients, incoming NIC frames are dropped silently by hardware. When clients release buffers via `NetOp::ReleaseBuffer`, `virtio_net_server` immediately replenishes `RX_VQ`.

### 5. Stable Network Service IPC Interface (`gaxera_abi::net`)
- `gaxera_abi::net` defines stable IPC operation codes:
  ```rust
  pub enum NetOp {
      SendPacket = 1,
      ReceivePacket = 2,
      ReleaseBuffer = 3,
      GetMacAddress = 4,
      QueryStatus = 5,
  }
  ```

### 6. MAC Address Caching
- Device MAC address (6 bytes) is read **once** during driver initialization and cached in `virtio_net_server` memory. `GetMacAddress` IPC returns cached MAC bytes in $O(1)$ time without reading hardware config space.

---

## Explicit Non-Goals for ADR 0032

- Hardware Checksum Offload (`VIRTIO_NET_F_CSUM`) (deferred to v1.0).
- Segmentation Offload (`VIRTIO_NET_F_GUEST_TSO4`) (deferred to v1.0).
- Multiqueue networking (`VIRTIO_NET_F_MQ`).

---

## Consequences & Invariants

1. **Zero Memory Leaks:** Client termination automatically reclaims leased DMA packet buffers.
2. **Event-Driven Non-Blocking IPC:** WaitSet notification integration provides high-performance asynchronous packet processing.
3. **Zero-Copy Performance:** Shared `Mapping` frame capabilities eliminate packet copy overhead across user processes.
