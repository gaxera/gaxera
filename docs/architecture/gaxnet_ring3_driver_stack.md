# GaxNet Ring-3 Driver & Decoupled Service Architecture

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Provider Architecture](gaxnet_provider_architecture.md)

---

## 1. Decoupled User-Space Service Architecture

Gaxera enforces complete separation across 4 Ring-3 user-space services:

```
+-----------------------------------------------------------------------+
|                    Ring 0: Microkernel Core                           |
|  (Manages address spaces, capabilities, IPC, interrupts. NO NET CODE) |
+-----------------------------------------------------------------------+
        ▲                                                ▲
        | Capability IPC                                 | Interrupt Notification
        v                                                v
+------------------------------------+    +-----------------------------+
| Ring 3 Protocol Server             |    | Ring 3 VirtIO Net Server    |
| (`net_stack_server`)               |    | (`virtio_net_server`)       |
| - Layered Providers (TCP/UDP/QUIC) |    | - PCI Express BAR Mapping   |
| - IP Routing & ARP Table           |    | - Virtqueue Descriptor Ring |
+------------------------------------+    +-----------------------------+
        ▲                                                ▲
        | Transport IPC                                  | DNS IPC
        v                                                v
+------------------------------------+    +-----------------------------+
| Ring 3 Crypto Service (`crypto_server`) | Ring 3 Resolver Service     |
| - TLS 1.3 / DTLS Handshake Engine  |    | (`resolver_server`)         |
| - Certificate Store & Key Isolation|    | - DNS / DoH / mDNS Engine   |
+------------------------------------+    +-----------------------------+
```

---

## 2. Decoupled Service Responsibilities

### 2.1 `virtio_net_server` (Hardware Driver Service)
- PCI Express BAR mapping via `Mapping` capabilities.
- Generic frame transport over DMA `ContiguousFrame` physical memory allocations.
- Virtqueue descriptor ring management and doorbell I/O.

### 2.2 `net_stack_server` (Network Protocol Stack Service)
- Ethernet II framing, MAC validation, dynamic ARP cache.
- IPv4/IPv6 routing table evaluation and IP checksum computation.
- Transport state execution (`TransportInstance`) for TCP, UDP, and QUIC.

### 2.3 `resolver_server` (Domain Resolution Service)
- Domain name resolution (DNS), DNS-over-HTTPS (DoH), mDNS local discovery.
- Provenance tracking and cryptographic signature verification of resolution records.

### 2.4 `crypto_server` (Session Encryption & Certificate Service)
- Housed in an independent Ring-3 service, completely separate from `net_stack_server`.
- Owns private keys, certificates, session tickets, and TLS 1.3 / DTLS handshake negotiation.
