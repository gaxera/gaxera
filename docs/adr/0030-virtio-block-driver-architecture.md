# ADR 0030: VirtIO Block Driver Architecture (`virtio_block_server`)

> **Status:** Proposed  
> **Date:** 2026-07-23  
> **Initiative:** Milestone 0.9.3 — VirtIO Block Driver (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `crates/virtio_block_server`, `crates/libgaxera`, `crates/gaxera-abi`  

---

## Context & Problem Statement

Gaxera requires a ring-3 microkernel storage driver (`virtio_block_server`) to provide block storage read/write capabilities for file systems and user applications.

VirtIO Block Specification (VirtIO 1.0) requires:
1. Receiving BAR `Mapping` and IRQ `Interrupt` capabilities delegated by `pci_bus_server`.
2. Initializing VirtIO Modern PCI transport via `libgaxera::virtio`.
3. Formatting VirtIO Block Request headers (`VirtioBlkHeader`: type `VIRTIO_BLK_T_IN` = 0, `VIRTIO_BLK_T_OUT` = 1, `VIRTIO_BLK_T_FLUSH` = 4, sector offset).
4. Managing 3-part Virtqueue descriptor chains and DMA payload buffers up to queue depth.
5. Exposing block storage service IPC endpoints (`ENDPOINT_BLOCK_SERVICE`).

---

## Protocol Boundary Invariant

> **Strict Protocol Boundary Invariant:**  
> `virtio_block_server` is the protocol boundary between the generic block service IPC interface (`gaxera_abi::block`) and the VirtIO Block device protocol. Filesystems and applications never observe VirtIO-specific descriptor chains, feature negotiation, transport structures, or hardware registers.

---

## Technical Decisions

### 1. `BlockRequest` Lifetime & Ownership Model
- `virtio_block_server` manages an internal `PendingRequestMap` indexed by `head_idx: u16`.
- Each active transaction is encapsulated in an owned `BlockRequest` struct:
  ```rust
  pub struct BlockRequest {
      header: VirtioBlkHeader,
      dma_handle: ContiguousFrameHandle,
      status_byte: u8,
      caller_handle: Option<Handle>,
  }
  ```
- **Automatic Resource Reclamation:** When `BlockRequest` is dropped, `ContiguousFrameHandle` automatically releases DMA memory, preventing DMA resource leaks or dangling physical frames.

### 2. Queue Depth Concurrency Model
- `virtio_block_server` supports **multiple outstanding requests** up to the negotiated `Virtqueue::size()` (e.g. 64 or 128 concurrent transfers).
- Descriptor heads are allocated dynamically from the Virtqueue free list.

### 3. $O(1)$ Completion Matching via Descriptor Head Index
- When the VirtIO Used Ring returns `VirtioUsedElem { id: u32, len: u32 }`, the returned `id` corresponds exactly to `head_idx`.
- `virtio_block_server` removes `BlockRequest` from `PendingRequestMap.remove(&(id as u16))` in $O(1)$ time with zero dynamic allocations.

### 4. Generic Block Service IPC Interface (`gaxera_abi::block`)
- `gaxera_abi::block` defines stable IPC operation codes:
  ```rust
  pub enum BlockOp {
      ReadSectors = 1,
      WriteSectors = 2,
      Flush = 3,
      QueryCapacity = 4,
  }
  ```
- Filesystems issue IPC calls using `gaxera_abi::block` without knowledge of VirtIO headers or rings.

### 5. Standardized Error Translation Model (`BlockError`)
- Raw VirtIO status bytes are translated into transport-independent IPC errors before replying to clients:
  - `VIRTIO_BLK_S_OK` ($0$) $\rightarrow$ `BlockError::Success` ($0$)
  - `VIRTIO_BLK_S_IOERR` ($1$) $\rightarrow$ `BlockError::IoError` ($1$)
  - `VIRTIO_BLK_S_UNSUPP` ($2$) $\rightarrow$ `BlockError::UnsupportedOperation` ($2$)
  - Device Timeout / Panic $\rightarrow$ `BlockError::DeviceFailure` ($3$)

### 6. Shutdown & Recovery Cleanup
- On driver exit or hardware reset:
  1. Interrupt handling is disabled.
  2. Device is reset (`status = 0`).
  3. `PendingRequestMap` is drained, failing pending IPC callers with `BlockError::DeviceFailure`.
  4. Dropping `BlockRequest` instances automatically unmaps and releases all active `ContiguousFrameHandle` DMA frames.

---

## Consequences & Invariants

1. **Storage Isolation:** Disk request failures or hardware stalls affect only `virtio_block_server`, keeping the microkernel TCB isolated.
2. **Clean IPC Abstraction:** Filesystems interact strictly with generic `gaxera_abi::block` IPC calls.
3. **Zero Resource Leaks:** Single-owner `BlockRequest` guarantees safe DMA memory reclamation.
