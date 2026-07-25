# GaxNet Native Object Model & Transport Abstraction

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Namespace Architecture](gaxnet_namespace_spec.md), [Design Principles](gaxnet_design_principles.md)

---

## 1. Core Network Object Hierarchy

```
                            +------------------------+
                            |  GaxNetObject (Root)   |
                            +-----------+------------+
                                        |
     +---------------+------------------+------------------+---------------+
     |               |                  |                  |               |
     v               v                  v                  v               v
+------------+ +-------------+    +------------+    +-------------+  +------------+
|NetNamespace| |NetInterface |    | NetListener|    | NetSession  |  | NetEndpoint|
+------------+ +-------------+    +------------+    +-----+-------+  +------------+
                                        |                 |
                                        v                 v
                                  +------------+   +---------------+
                                  |  NetRoute  |   |TransportInst. |
                                  +------------+   +---------------+
```

### 2.1 `NetNamespace` (Network Isolation Scope Object)
First-class object encapsulating network interfaces, routes, listeners, sessions, and resolver configuration.

### 2.2 `NetInterface` (Network Interface Object)
Represents a physical or virtual network interface card (NIC).

### 2.3 `NetListener` (Passive Listening Object)
Separates passive listening from active communication sessions: owns bind address (`NetEndpoint`), listen queue backlog, and accepts new `NetSession` capabilities.

### 2.4 `NetEndpoint` (Endpoint Identifier Object)
Decouples network addressing (IP, Port, Protocol Family, Interface Binding) from transport state. Transport migration or multipath networking does not alter application session handles.

### 2.5 `NetSession` (Active Communication Session Object)
Represents an established communication session. `NetSession` owns zero protocol-specific code; it delegates runtime state execution to an internal `TransportInstance` reference.

### 2.6 `TransportInstance` (Protocol State Object)
Created and owned by a `TransportProvider`. Encapsulates internal protocol state machine variables (e.g. TCP sequence numbers, congestion windows, QUIC connection IDs) away from applications.

### 2.7 `PacketRing` (Shared Memory Packet Ring Object)
Page-aligned zero-copy shared memory buffer pair carrying generic `FrameDescriptor` entries.

### 2.8 `NetRoute` (Routing Table Entry Object)
Represents a network route policy mapping destination IP prefix ranges to target gateways and interfaces.
