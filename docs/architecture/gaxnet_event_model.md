# GaxNet Event-Driven Architecture & Unified OS Event Framework

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Design Principles](gaxnet_design_principles.md)

---

## 1. Separation of Kernel vs User-Space Event Responsibilities

GaxNet strictly separates low-level microkernel event delivery from high-level user-space protocol event semantics:

### **Kernel Ring-0 Event Responsibilities:**
- Hardware MSI-X interrupt delivery to user-space driver endpoints.
- Microkernel `WaitSet` signal multiplexing & atomic state-word notifications.
- Shared-memory `PacketRing` buffer readiness notifications.
- Inter-Process Communication (IPC) message queuing signals.

### **User-Space Ring-3 Event Responsibilities:**
- Protocol state transitions (`SessionConnected`, `SessionClosed`).
- Packet IO events (`PacketReceived`, `PacketTransmitted`).
- Network topology updates (`RouteChanged`, `InterfaceStateChanged`).
- Domain resolution events (`DnsResolved`).

---

## 2. Unified OS-Wide Event Taxonomy

GaxNet's event architecture integrates natively into Gaxera's universal operating system event framework:

```
                          +------------------------+
                          |   GaxOsEvent (Root)    |
                          +-----------+------------+
                                      |
     +--------------+-------------+---+-------------+--------------+
     |              |             |                 |              |
     v              v             v                 v              v
+---------+    +---------+   +---------+       +---------+    +---------+
| Network |    | Storage |   |   IPC   |       | Display |    | System  |
+---------+    +---------+   +---------+       +---------+    +---------+
```

### Universal Event Variant Taxonomy:

```rust
pub enum GaxOsEvent {
    Network(NetEvent),
    Storage(StorageEvent),
    Ipc(IpcEvent),
    Display(DisplayEvent),
    System(SystemEvent),
}

pub enum NetEvent {
    PacketReceived { session_id: GaxObjectId, bytes: u32 },
    PacketTransmitted { session_id: GaxObjectId, bytes: u32 },
    SessionConnected { session_id: GaxObjectId, remote: NetEndpoint },
    SessionClosed { session_id: GaxObjectId, reason: CloseReason },
    InterfaceStateChanged { interface_id: GaxObjectId, link_up: bool },
    RouteChanged { route_id: GaxObjectId },
    DnsResolved { domain: String, addresses: Vec<IpAddr> },
}
```

---

## 3. High-Performance Reactor Pattern (`WaitSet` Integration)

1. **Atomic Signal Words:** Every `NetSession` and `NetListener` maintains a lock-free atomic state bitmask accessible by user applications via read-only shared memory.
2. **Universal Multiplexing:** Applications register `NetSession`, `StorageJournal`, `IpcEndpoint`, `TimerObject`, and `WindowHandle` descriptors into a single microkernel `WaitSet`.
3. **Zero-Poll Reactor:** Application threads sleep on `WaitSet` atomic wait, waking instantly when any registered subsystem emits a notification.
