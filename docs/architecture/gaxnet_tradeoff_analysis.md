# GaxNet Trade-Off Analysis

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Packet Ownership](gaxnet_packet_ownership.md)

---

## 1. Trade-Off Evaluation Matrix

### 1.1 Microkernel Isolation vs. Throughput Latency
- **Trade-Off:** Executing drivers (`virtio_net_server`) and protocol processors (`net_stack_server`) in Ring 3 introduces IPC context switch overhead compared to monolithic in-kernel networking.
- **Resolution:** GaxNet utilizes page-aligned `PacketRing` shared-memory buffers. Applications read and write packet payloads directly without microkernel IPC calls during data transfer. Microkernel IPC is invoked strictly during session setup and capability derivation.

### 1.2 Capability Security vs. POSIX Compatibility
- **Trade-Off:** Restricting network operations via capability tokens (`NetRights`) breaks legacy C applications expecting ambient socket creation (`socket(AF_INET, SOCK_STREAM, 0)`).
- **Resolution:** `libgaxera::compat::sockets` maps legacy BSD socket calls to GaxNet capabilities. At application launch, the Supervisor injects required network capabilities into the process CSpace based on declared manifest permissions.

---

## 2. Quantitative Performance Targets

| Metric | Target | Rationale |
| --- | --- | --- |
| **Packet Copying** | **0 bytes / payload** | Payload memory frames are passed via `PacketRing` pointers. |
| **IPC Setup Latency** | **< 1.5 μs** | Session capability derivation and handshake latency. |
| **Packet RX/TX Latency** | **< 5 μs** | Doorbell IRQ to application notification processing time. |
| **Memory Footprint** | **< 4 MB** | Complete `net_stack_server` Ring-3 RSS memory budget. |
