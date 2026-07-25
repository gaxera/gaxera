# GaxNet Service Failure Domains & Restart Sequences

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Specification](gaxnet_specification.md)

---

## 1. Granular Service Failure Domains

GaxNet defines strict failure domain boundaries for each Ring-3 network service:

```
+-----------------------------------------------------------------------+
|                    GaxNet Service Failure Domains                     |
+-----------------------------------------------------------------------+
|                                                                       |
|  [virtio_net_server]  ──► Crashes? Only driver restarts.              |
|                           `net_stack_server` buffers frames.          |
|                                                                       |
|  [net_stack_server]   ──► Crashes? Protocol engine restarts.          |
|                           `crypto_server` retains cert keys.          |
|                                                                       |
|  [resolver_server]    ──► Crashes? DNS cache resets.                  |
|                           Active network sessions continue.           |
|                                                                       |
|  [crypto_server]      ──► Crashes? Crypto session state resets.       |
|                           Private identity keys safe in storage.      |
|                                                                       |
+-----------------------------------------------------------------------+
```

---

## 2. Service Restart & Recovery Sequences

### 2.1 `virtio_net_server` Crash Recovery Sequence
1. **Crash Detection:** Supervisor detects driver exit via `WaitSet` notification.
2. **Surviving Services:** `net_stack_server`, `resolver_server`, `crypto_server`, and user applications remain active.
3. **Preserved State:** `NetSession` capabilities and `PacketRing` buffers in `net_stack_server` remain mapped.
4. **Restart Execution:** Supervisor launches new `virtio_net_server` instance (< 1 ms).
5. **Re-Initialization:** Driver re-maps PCI BAR, re-negotiates Virtqueue descriptors, and re-binds MSI-X IRQ capabilities.
6. **Queue Flush:** Buffered TX frames are transmitted immediately once link status returns to `Ready`.

### 2.2 `net_stack_server` Crash Recovery Sequence
1. **Crash Detection:** Supervisor detects `net_stack_server` process exit.
2. **Surviving Services:** `virtio_net_server` holds hardware queues; `crypto_server` retains private keys.
3. **Restart Execution:** Supervisor launches new `net_stack_server` instance.
4. **Re-Registration:** Stack process re-registers `NetworkProvider` and `TransportProvider` manifests with Service Registry.
5. **Session Teardown:** Application session handles emit `NetEvent::SessionClosed { reason: ProviderCrashed }`. Applications reconnect cleanly.

### 2.3 `resolver_server` Crash Recovery Sequence
1. **Crash Detection:** Supervisor detects `resolver_server` exit.
2. **Surviving Services:** All active network sessions and transport connections continue uninterrupted.
3. **Restart Execution:** Supervisor restarts `resolver_server`. DNS queries during restart return `Err(NetError::ResolverUnavailable)`.

### 2.4 `crypto_server` Crash Recovery Sequence
1. **Crash Detection:** Supervisor detects `crypto_server` exit.
2. **Surviving Services:** Unencrypted network connections continue. Active TLS sessions emit `CloseReason::CryptoProviderCrashed`.
3. **Restart Execution:** Supervisor restarts `crypto_server`, reloads identity certificates from GaxFS storage, and resumes TLS 1.3 handshake capability.
