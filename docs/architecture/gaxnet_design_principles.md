# GaxNet First Design Principles

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Master Specification](gaxnet_specification.md)

---

## Executive Summary

GaxNet is not a port of legacy BSD sockets into user space. It is a ground-up, first-principles networking platform engineered specifically for Gaxera's microkernel architecture, capability security model, zero-copy IPC framework, and event-driven OS design.

---

## Fundamental Design Principles

### 1. Networking is Object-Oriented
Network constructs are represented by typed, first-class kernel and user-space objects (`NetInterface`, `NetListener`, `NetSession`, `NetEndpoint`, `PacketRing`, `NetRoute`). Applications interact with explicit object handles rather than opaque integer file descriptors (`int fd`).

### 2. Authority is Explicit (Zero Ambient Network Authority)
There is no ambient right to open sockets, listen on ports, or transmit packets. Access to any network resource requires an explicit `CapabilityHandle` carrying subsystem-specific `NetRights`. Network access is denied by default unless granted by application capability manifests.

### 3. Data Movement is Minimized (Zero-Copy Data Pipeline)
Data transfers between hardware drivers, network protocol engines, and application processes utilize page-aligned shared-memory `PacketRing` buffers. Packet payload data is never copied across process boundaries.

### 4. Protocols are Replaceable (Layered Provider Architecture)
The networking subsystem contains no fixed protocol implementations in its core interface. Device drivers, link layers, network routing, transport protocols, domain resolvers, and crypto engines are defined as abstract provider traits implemented in unprivileged Ring-3 user-space services.

### 5. Compatibility Layers Do Not Define Native Architecture
Interoperability with existing Internet standards (Ethernet, IPv4/IPv6, TCP, UDP, DNS, TLS) is strictly preserved. However, legacy POSIX/BSD socket APIs exist exclusively as a Ring-3 virtualization wrapper (`libgaxera::compat::sockets`). The native architecture takes absolute precedence over POSIX abstractions.

### 6. Event-Driven Communication Precedes Polling
GaxNet replaces legacy polling loops (`select`, `poll`, `epoll`) with native, event-driven reactive signals. Microkernel `WaitSet` primitives multiplex hardware notifications, packet arrivals, and session state transitions directly to application event reactors without polling debt.

### 7. Transport is Independent of Application Semantics
Network sessions (`NetSession`) decouple application identity from underlying transport state (TCP, UDP, QUIC). Applications interact with durable session objects without embedding protocol-specific state machines into application code.

### 8. Architectural Subsystem Independence
GaxNet shares foundational philosophy with GaxFS (capabilities, providers, event-driven design, object orientation) without sharing subsystem-specific types, rights, or abstractions. GaxNet stands as an independent peer architecture to GaxFS.
