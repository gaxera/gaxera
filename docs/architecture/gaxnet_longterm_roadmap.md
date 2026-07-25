# GaxNet Long-Term Innovation Roadmap

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Design Principles](gaxnet_design_principles.md)

---

## 1. Technological Innovation Horizon

GaxNet's decoupling of network transports from legacy kernel sockets enables rapid integration of cutting-edge networking innovations:

```
+-----------------------------------------------------------------------+
|                    GaxNet Future Innovation Series                    |
+-----------------------------------------------------------------------+
|                                                                       |
|  [v1.0] Native QUIC Transport & Encrypted TLS 1.3 Acceleration       |
|                                                                       |
|  [v1.1] Native IPv6-First Architecture & Segment Routing (SRv6)       |
|                                                                       |
|  [v1.2] Capability-Gated eBPF Packet Filter Offloading                |
|                                                                       |
|  [v1.3] Zero-Trust Mesh Peer-to-Peer Networking (WireGuard Protocol)  |
|                                                                       |
+-----------------------------------------------------------------------+
```

---

## 2. Milestone Extensions

### 2.1 Native QUIC & HTTP/3 Provider (v1.0)
- Integrates UDP-based QUIC transport directly as a first-class `TransportProvider`.
- Eliminates head-of-line blocking and provides zero-RTT session resumption natively.

### 2.2 Native IPv6-First Architecture (v1.1)
- 128-bit IPv6 addressing supported as default primary IP type across all `NetInterface` objects.
- Stateless Address Autoconfiguration (SLAAC) and Neighbor Discovery Protocol (NDP).

### 2.3 Capability-Gated Packet Filter Offload (v1.2)
- Safe, sandboxed bytecode packet filtering engine offloaded into Ring-3 `virtio_net_server` for hardware-speed packet drop, rate limiting, and firewall inspection.

### 2.4 Zero-Trust Mesh Protocol (v1.3)
- Embedded WireGuard protocol engine establishing authenticated, encrypted peer-to-peer tunnels between Gaxera nodes automatically.
