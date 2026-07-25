# GaxNet Layered Provider Architecture & Common Lifecycle

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Design Principles](gaxnet_design_principles.md)

---

## 1. Common Provider Lifecycle State Machine

Every GaxNet provider (`DeviceProvider`, `LinkProvider`, `NetworkProvider`, `TransportProvider`, `CryptoProvider`, `ResolverProvider`) obeys a common lifecycle state machine:

```
  +------------+       +------------+       +-------------+       +-------+
  | Discovered | ───►  | Registered | ───►  | Initialized | ───►  | Ready |
  +------------+       +------------+       +-------------+       +---+---+
                                                                      |
                                                                      v
   +---------+         +------------+       +----------+          +-------+
   | Stopped | ◄────── | Restarting | ◄──── | Degraded | ◄─────── |Running|
   +---------+         +------------+       +----------+          +-------+
```

### Lifecycle State Invariants:
1. **`Discovered`:** Provider process detected by Supervisor; binary verified.
2. **`Registered`:** IPC manifest registered with Service Registry; capability rights validated.
3. **`Initialized`:** Hardware BAR / memory queues allocated; IPC version negotiated (`version: u32`).
4. **`Ready`:** Dependencies resolved across provider layers.
5. **`Running`:** Active data plane packet processing enabled.
6. **`Degraded`:** Non-fatal issue detected (e.g. packet loss burst); fallback policies active.
7. **`Restarting`:** Provider crashed or requested restart; descriptors reclaimed to `FREE` pool.
8. **`Stopped`:** Process cleanly terminated; resources unmapped.

---

## 2. IPC Protocol Version Negotiation

Every provider IPC interface mandates explicit protocol version negotiation at the first message header:

```rust
#[repr(C)]
pub struct ProviderIpcHeader {
    pub protocol_magic: u32,  // Magic identifier (e.g. 0x4741584E = b"GAXN")
    pub protocol_version: u32,// Protocol version (e.g. 1)
    pub message_type: u16,
    pub message_len: u32,
}
```

- **Backward Compatibility Guarantee:** Higher minor version providers support lower minor version clients. Major version mismatches fail initialization cleanly with `Err(IpcError::VersionMismatch)`.

---

## 3. Layered Provider Composition Pipeline

```
+-----------------------------------------------------------------------+
|                    GaxNet Layered Composition Pipeline                |
+-----------------------------------------------------------------------+
|  [Layer 1: DeviceProvider]    (Hardware DMA & Virtqueue IRQ Frames)   |
|            │                                                          |
|            ▼                                                          |
|  [Layer 2: LinkProvider]      (Ethernet II Framing & ARP Resolution)  |
|            │                                                          |
|            ▼                                                          |
|  [Layer 3: NetworkProvider]   (IPv4 / IPv6 Route Lookup & Addressing) |
|            │                                                          |
|            ▼                                                          |
|  [Layer 4: TransportProvider] (TCP / UDP / QUIC TransportInstances)   |
|            │                                                          |
|            ▼                                                          |
|  [Layer 5: CryptoProvider]    (TLS 1.3 / DTLS Payload Encryption)    |
|            │                                                          |
|            ▼                                                          |
|  [Layer 6: ResolverProvider]  (DNS / mDNS Domain Name Resolution)     |
+-----------------------------------------------------------------------+
```
