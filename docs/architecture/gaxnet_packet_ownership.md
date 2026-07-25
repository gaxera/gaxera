# GaxNet Packet Ownership & PacketRing Backpressure Policies

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Object Model](gaxnet_object_model.md), [Design Principles](gaxnet_design_principles.md)

---

## 1. Transport-Independent Frame Transport

`PacketRing` is a generic, transport-independent frame transport mechanism. It carries generic **`FrameDescriptor`** structures pointing to payload memory frames:

```
+-------------------------------------------------------------------------+
|                      Page-Aligned Shared Memory Ring                    |
|                                                                         |
|  +--------------------+------------------------+---------------------+  |
|  | FrameDescriptor    | Frame Type Flags       | Payload Memory Frame|  |
|  | (32 bytes)         | (Ethernet/Virtual/TLS) | (DMA Contiguous)    |  |
|  +--------------------+------------------------+---------------------+  |
+-------------------------------------------------------------------------+
```

---

## 2. `PacketRing` Backpressure Policies

GaxNet specifies explicit backpressure policies for each ring type when capacity is reached:

| Ring Type | Ring Role | Primary Backpressure Policy | Secondary / Fallback Policy | Behavioral Guarantee |
| --- | --- | --- | --- | --- |
| **RX Ring** (Driver $\rightarrow$ Stack $\rightarrow$ App) | Inbound payload arrival | **Flow Control Notification** | **Drop Oldest** | Signal transport TCP window shrink; drop oldest unconsumed frame if queue fills under extreme burst. |
| **TX Ring** (App $\rightarrow$ Stack $\rightarrow$ Driver) | Outbound payload submission | **Block Producer** | **Drop Newest** | Block application thread until slot frees; drop newest frame if non-blocking socket option is set. |
| **Control Ring** (Management / Configuration) | Service control messages | **Priority Discard** | **N/A** | Low-priority metrics discarded; high-priority teardown/reconfiguration messages guaranteed delivery. |

---

## 3. Formal Packet Ownership State Machine

```
               +------+
               | FREE | ◄─────────────────────────────────────────────+
               +--+---+                                              |
                  |                                                  | Reclaim /
                  v (DMA Setup)                                      | Release
              +--------+                                             |
              | DMA_RX |                                             |
              +---+----+                                             |
                  |                                                  |
                  v (IRQ Trigger)                                    |
              +--------+                                             |
              | Driver |                                             |
              +---+----+                                             |
                  |                                                  |
                  v (PacketRing Handoff)                             |
          +---------------+                                          |
          |Protocol Stack |                                          |
          +-------+-------+                                          |
                  |                                                  |
                  v (PacketRing Consumer)                            |
            +-------------+                                          |
            | Application |                                          |
            +-----+-------+                                          |
                  |                                                  |
                  v (TX Submission)                                  |
          +---------------+                                          |
          |Protocol Stack |                                          |
          +-------+-------+                                          |
                  |                                                  |
                  v (Virtqueue TX)                                   |
              +--------+                                             |
              | DMA_TX | ────────────────────────────────────────----+
              +--------+
```

---

## 4. `PacketRing` Architectural Invariants

1. **Single Producer / Single Consumer (SPSC):** Exactly one producer (`WRITE`) and one consumer (`READ`) per ring index.
2. **Immutable Published Descriptors:** Descriptors become strictly immutable once published.
3. **Unique Payload Ownership:** Payload frame addresses cannot be duplicated across multiple descriptors.
4. **Bounded Capacity:** Power-of-two modulo-free bitwise AND index wraparound.
5. **Descriptor & Payload Lifetime Guarantees:** Physical pages remain locked while descriptors are active.
