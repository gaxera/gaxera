# ADR 0035: GaxNet First-Principles Architecture

## Status
Accepted

## Context
Traditional operating system networking architectures (BSD sockets, Linux `sock`/`sk_buff`, Winsock) were designed for monolithic kernels running on single-core hardware without capability security, zero-copy IPC, or microkernel isolation.

### Architectural Invariant:
> **"The kernel contains no network protocol parsing or network device driver logic."**

### Precise Boundary Definition:

#### Kernel Ring-0 Responsibilities:
- Interrupt delivery & MSI-X routing
- Scheduler thread wakeups
- Capability security enforcement (`CapabilityHandle`)
- Inter-Process Communication (IPC) transports
- Page-aligned memory mapping & address space isolation
- DMA-safe physical memory management (`ContiguousFrame`)
- Microkernel `WaitSet` event notifications

#### Decoupled Ring-3 Service Boundaries:
- `virtio_net_server`: Hardware PCI BAR mapping, Virtqueues, DMA, doorbell IRQs.
- `net_stack_server`: Ethernet, ARP, IPv4/IPv6 routing, TCP, UDP, QUIC.
- `resolver_server`: Dedicated DNS, DoH, mDNS service discovery & DNS cache.
- `tls_server`: Dedicated TLS 1.3 / DTLS session encryption, certificate management, key isolation.
- `Applications`: Business logic & native networking APIs inside isolated `NetNamespace` objects.

## Decision
Gaxera adopts **GaxNet (Gaxera Native Network Platform)**, a first-principles network architecture designed specifically for microkernel capability isolation, zero-copy shared-memory IPC, event-driven reactive scheduling, and protocol agility.

### Key Architectural Choices:

1. **Strict Control Plane vs. Data Plane Separation:**  
   - **Control Plane:** Session setup, routing updates, capability verification, namespace configuration, provider registration via microkernel IPC.
   - **Data Plane:** Packet payload transfer over zero-copy `PacketRing` shared memory with zero control IPC overhead.

2. **First-Class `NetNamespace` Object:**  
   Applications operate inside an explicit `NetNamespace` capability object encapsulating interfaces, routes, listeners, sessions, and resolver configurations.

3. **Dedicated `resolver_server` & `crypto_server` Services:**  
   DNS and TLS operations are completely decoupled from `net_stack_server` into independent Ring-3 services.

4. **Common Provider Lifecycle & Dynamic Registration:**  
   All network providers follow a standard lifecycle: `Discovered` $\rightarrow$ `Registered` $\rightarrow$ `Initialized` $\rightarrow$ `Ready` $\rightarrow$ `Running` $\rightarrow$ `Degraded` $\rightarrow$ `Restarting` $\rightarrow$ `Stopped`. Every IPC interface mandates protocol version negotiation (`version: u32`).

5. **Authoritative Network State Ownership:**  
   Every network state object has exactly one authoritative Ring-3 owner:
   - Routing Table & IP Routes $\rightarrow$ `net_stack_server`
   - Neighbor Cache (ARP/NDP) $\rightarrow$ `net_stack_server`
   - Interface Config $\rightarrow$ `net_stack_server`
   - DNS Resolution Store $\rightarrow$ `resolver_server`
   - Transport State (`TransportInstance`) $\rightarrow$ `net_stack_server`
   - TLS Certificates & Identity Keys $\rightarrow$ `crypto_server`
   - Capability Tree & Rights $\rightarrow$ Microkernel Ring 0

6. **Defined `PacketRing` Backpressure Policies:**  
   Explicit backpressure strategies per ring type:
   - RX Ring: Flow Control Notification / Drop Oldest
   - TX Ring: Block Producer / Drop Newest
   - Control Ring: Priority Discard

7. **Granular Service Failure Domains & Restart Sequences:**  
   Service failure domains guarantee zero kernel panic or resource leak during process restarts.

## Security Claims & Attack Surface Reduction

Gaxera makes precise, architecturally verifiable security statements:
- **Kernel Attack Surface Reduction:** Moving network device drivers and protocol parsers to Ring 3 significantly reduces kernel Ring-0 attack surface.
- **Protocol Fault Containment:** A remote exploitation or panic in a Ring-3 network protocol service does not directly compromise kernel execution or escalate Ring-0 privileges. The Supervisor detects crashes and restarts the process in < 1 ms.
- **Blast-Radius Bounding:** Subsystem capability isolation (`NetRights`) restricts compromised user-space services to their explicitly granted policy scopes.

## References
- [GaxNet Design Principles](../architecture/gaxnet_design_principles.md)
- [GaxNet Master Specification](../architecture/gaxnet_specification.md)
- [GaxNet Object Model](../architecture/gaxnet_object_model.md)
- [GaxNet Namespace Architecture](../architecture/gaxnet_namespace_spec.md)
- [GaxNet Capability Model](../architecture/gaxnet_capability_model.md)
- [GaxNet Event Model](../architecture/gaxnet_event_model.md)
- [GaxNet Packet Ownership](../architecture/gaxnet_packet_ownership.md)
- [GaxNet Provider Architecture](../architecture/gaxnet_provider_architecture.md)
- [GaxNet Failure Semantics](../architecture/gaxnet_failure_semantics.md)
- [GaxNet Compatibility Strategy](../architecture/gaxnet_compatibility_strategy.md)
- [GaxNet Ring-3 Driver Stack](../architecture/gaxnet_ring3_driver_stack.md)
