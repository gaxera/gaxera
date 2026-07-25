# GaxNet Master Architectural Specification

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Design Principles](gaxnet_design_principles.md), [Namespace Architecture](gaxnet_namespace_spec.md), [Failure Semantics](gaxnet_failure_semantics.md)

---

## 1. Control Plane vs Data Plane Architecture

GaxNet establishes a strict separation between Control Plane and Data Plane execution:

```
+-----------------------------------------------------------------------+
|                            CONTROL PLANE                              |
|  - Capability verification & policy enforcement                       |
|  - Session creation & NetNamespace isolation                          |
|  - Routing table updates & interface configuration                    |
|  - Provider dynamic registration & discovery                          |
|  (Executes via microkernel IPC calls)                                 |
+-----------------------------------------------------------------------+

=========================================================================

+-----------------------------------------------------------------------+
|                             DATA PLANE                                |
|  - DMA physical frame allocations (`ContiguousFrame`)                 |
|  - Zero-copy payload transfer over `PacketRing` shared memory         |
|  - In-place protocol parsing (Ethernet, IPv4/IPv6, TCP, UDP, QUIC)    |
|  - Atomic `WaitSet` event notifications                               |
|  (Executes with ZERO control IPC overhead during data transfer)       |
+-----------------------------------------------------------------------+
```

---

## 2. Decoupled Service Boundary Architecture

```
+-----------------------------------------------------------------------+
|                    Application NetNamespace Scope                     |
|                                                                       |
|  +---------------------------+     +-------------------------------+  |
|  | Native GaxNet API         |     | POSIX Socket Compat Layer     |  |
|  | (PacketRing / NetSession) |     | (libgaxera::compat::sockets)  |  |
|  +-------------+-------------+     +---------------+---------------+  |
+----------------|-----------------------------------|------------------+
                 | Domain Resolve IPC                | Transport IPC
                 v                                   v
+------------------------------------+   +------------------------------+
| Ring-3 Resolver Service            |   | Ring-3 Crypto Service        |
| (`resolver_server`)                |   | (`crypto_server`)            |
| - DNS / DoH / mDNS Engine          |   | - TLS 1.3 / DTLS Encryption  |
| - Domain Cache & Provenance        |   | - Certificate & Key Isolation|
+-----------------+------------------+   +--------------+---------------+
                  |                                     |
                  +------------------+------------------+
                                     | Transport Stream IPC
                                     v
+-----------------------------------------------------------------------+
|          Ring-3 Network Protocol Server (`net_stack_server`)          |
|  - Ethernet / ARP / IPv4 / IPv6 Router                                |
|  - Transport Providers (TCP / UDP / QUIC)                             |
+-----------------------------------------------------------------------+
                                     | Zero-Copy Frame Ring
                                     v
+-----------------------------------------------------------------------+
|         Ring-3 VirtIO Net Driver Server (`virtio_net_server`)         |
|  - Virtqueue Descriptor Ring Management & Hardware Doorbell I/O       |
+-----------------------------------------------------------------------+
```

---

## 3. Authoritative Network State Ownership Matrix

| Network State Object | Authoritative Ring-3 Owner | Shared Access Policy |
| --- | --- | --- |
| **Routing Table (`NetRoute` array)** | `net_stack_server` | Read-only copy published to sub-namespaces. |
| **Neighbor Cache (ARP / NDP)** | `net_stack_server` | Internal private state. |
| **Interface Config (`NetInterface`)** | `net_stack_server` | Status view exported to applications via IPC. |
| **DNS Resolution Cache** | `resolver_server` | Query resolution exported to applications. |
| **Transport State (`TransportInstance`)** | `net_stack_server` | Session capability exported to application CSpace. |
| **TLS Certificates & Identity Keys** | `crypto_server` | Private keys NEVER exported to any process. |
| **Capability Tree & Rights** | Microkernel Ring 0 | Read-only capability handle validation. |

---

## 4. Specification Suite Index

GaxNet is documented across **16 canonical architectural specifications**:

1. [GaxNet Design Principles](gaxnet_design_principles.md): Philosophical principles.
2. [GaxNet Network Namespace](gaxnet_namespace_spec.md): First-class `NetNamespace` architecture.
3. [GaxNet Object Model](gaxnet_object_model.md): Core object hierarchy, `TransportInstance`, `NetListener`, `NetEndpoint`.
4. [GaxNet Capability Model](gaxnet_capability_model.md): Subsystem `NetRights` & capability delegation trees.
5. [GaxNet Event Model](gaxnet_event_model.md): Universal OS event taxonomy & `WaitSet` reactor.
6. [GaxNet Packet Ownership](gaxnet_packet_ownership.md): Generic `FrameDescriptor`, `PacketRing` invariants, backpressure.
7. [GaxNet Provider Architecture](gaxnet_provider_architecture.md): 6 Layered Providers, lifecycle, IPC versioning.
8. [GaxNet Failure Semantics](gaxnet_failure_semantics.md): Failure domains, 10 recovery invariants, restart sequences.
9. [GaxNet Compatibility Strategy](gaxnet_compatibility_strategy.md): BSD Sockets and POSIX translation layer.
10. [GaxNet Ring-3 Driver Stack](gaxnet_ring3_driver_stack.md): VirtIO-Net driver and service boundaries.
11. [GaxNet Long-Term Roadmap](gaxnet_longterm_roadmap.md): QUIC, TLS 1.3, IPv6, WireGuard roadmap.
12. [GaxNet Comparative Research](gaxnet_comparative_research.md): Comparative analysis against BSD, io_uring, Fuchsia Netstack3.
13. [GaxNet Trade-Off Analysis](gaxnet_tradeoff_analysis.md): Trade-off evaluation.
14. [GaxNet On-Disk Storage Format](gaxnet_ondisk_format.md): State persistence and certificate storage format.
15. [GaxNet Master Specification](gaxnet_specification.md): System overview.
