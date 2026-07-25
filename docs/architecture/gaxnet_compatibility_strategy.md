# GaxNet POSIX & BSD Sockets Compatibility Strategy

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Design Principles](gaxnet_design_principles.md)

---

## 1. Compatibility Isolation Principle

GaxNet enforces **Native Architectural Precedence**:
- Native Gaxera applications use GaxNet `NetSession`, `NetListener`, `NetEndpoint`, and `PacketRing` capability handles directly.
- The POSIX BSD socket API (`socket()`, `bind()`, `listen()`, `accept()`, `connect()`, `send()`, `recv()`, `select()`, `epoll()`) exists strictly as a Ring-3 user-space virtualization layer in `libgaxera::compat::sockets`.
- The native microkernel IPC and `net_stack_server` protocol engine contain **zero legacy BSD socket code**.

---

## 2. Virtualization Mapping

```
+-------------------------------------------------------------------------+
|                  Legacy POSIX Application / C Binary                    |
|             (Invokes socket(), bind(), connect(), epoll())              |
+-------------------------------------------------------------------------+
                                    │
                                    ▼
+-------------------------------------------------------------------------+
|         Ring-3 POSIX Compatibility Wrapper (`libgaxera::compat`)        |
|                                                                         |
|  - Virtual Descriptor Table (`int fd` -> `GaxObjectId`)                 |
|  - `socket()` -> Allocates `PacketRing` & requests `NetSession` cap     |
|  - `bind()` / `listen()` -> Allocates native `NetListener` handle       |
|  - `connect()` -> Invokes `net_stack_server` IPC handshake              |
|  - `send()` / `recv()` -> Writes / Reads `PacketRing` shared memory     |
|  - `epoll_wait()` -> Multiplexes native `WaitSet` notifications         |
+-------------------------------------------------------------------------+
                                    │
                                    ▼
+-------------------------------------------------------------------------+
|                    Native GaxNet Protocol Engine                        |
|                         (`net_stack_server`)                            |
+-------------------------------------------------------------------------+
```

---

## 3. `epoll` / `select` Virtualization

1. **Virtual Descriptor Table:** The compatibility library maintains a thread-safe mapping table converting integer descriptors (`fd 3`, `fd 4`) to underlying `NetSession` or `NetListener` capabilities.
2. **`epoll_create()`:** Creates an internal `WaitSet` handle.
3. **`epoll_ctl()`:** Registers native session notification signals with the `WaitSet`.
4. **`epoll_wait()`:** Performs a zero-allocation `WaitSet` atomic wait, converting triggered GaxNet notifications back into `struct epoll_event` arrays for legacy C applications.
