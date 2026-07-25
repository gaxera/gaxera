# GaxNet Comparative Research & Architectural Analysis

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Specification](gaxnet_specification.md)

---

## 1. Architectural Matrix Comparison

| Design Metric | BSD Sockets (POSIX) | Linux `io_uring` | Fuchsia Netstack3 | GaxNet (Gaxera) |
| --- | --- | --- | --- | --- |
| **Execution Domain** | Ring 0 Monolithic Kernel | Ring 0 Kernel System Calls | Ring 3 User Space (Fuchsia) | **Ring 3 Microkernel Isolation** |
| **Access Control Model** | Ambient Path / UID Authority | Ambient Process Authority | Capability Handles (Fuchsia) | **Zero Ambient Authority + `NetRights` & Policy Scoping** |
| **Object Representation** | Opaque `int fd` | Ring Submission / Completion Queue | FIDL IPC Protocols | **First-Class `NetSession`, `NetListener`, `NetEndpoint`, `PacketRing` Capabilities** |
| **Data Copy Overhead** | Multiple `memcpy` calls | Zero-Copy (Registered Buffers) | FIDL Channel Copies | **Zero-Copy Shared Memory Ring Buffers (`PacketRing`)** |
| **Event Multiplexing** | `select` / `poll` / `epoll` | Ring Completion Polling | Zircon Signals & Async | **Microkernel `WaitSet` Reactor Signals & OS-Wide Taxonomy** |
| **Protocol Extensibility** | Difficult (Requires Kernel Modules) | Complex (Kernel Drivers) | Modular Rust Netstack | **6 Layered Provider Traits (`Device`, `Link`, `Network`, `Transport`, `Resolver`, `Crypto`)** |

---

## 2. Key Synthesis & Trade-Off Lessons

1. **Lesson from Fuchsia Netstack3:** Moving netstack to Ring 3 in Rust provides total crash resilience, but IPC serialization overhead can become a bottleneck unless zero-copy shared memory is used. GaxNet uses `PacketRing` shared memory to overcome this.
2. **Lesson from `io_uring`:** Asynchronous completion queues provide substantial throughput gains over `epoll`. GaxNet adopts ring-buffer descriptors for packet handoff without borrowing monolithic kernel Ring-0 vulnerabilities.
