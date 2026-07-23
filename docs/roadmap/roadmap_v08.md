# Gaxera v0.8 Epoch Roadmap: Hardware Architecture, Driver Foundation & Userspace Runtime

> **Status:** Draft / Active Planning  
> **Baseline:** Gaxera v0.7, tag `v0.7.0`  
> **Target:** Gaxera v0.8.0  
> **Primary Initiatives:** Driver & Hardware Architecture (`Program C`), Userspace Runtime (`Program E`), IPC Performance  

---

## 1. Executive Direction

v0.8 establishes the hardware driver foundation, capability-mediated interrupt delivery model, and user-space runtime environment (`libgaxera`) required for Gaxera to run user-mode device drivers and microkernel services without ambient authority.

Where v0.7 built a high-performance $N:1$ multi-client IPC engine with event multiplexing (`WaitSet`) and $O(1)$ priority-aware scheduling, v0.8 connects user-mode services directly to physical hardware, provides a safe C/Rust userspace runtime (`libgaxera`), and implements capability-based service discovery.

### Key Objectives
1. **Interrupt & Notification Architecture (v0.8.1):** First-class `InterruptObject` and `Notification` capabilities, IOAPIC programming, vector allocation, IRQ masking/acknowledgement, and seamless integration with `WaitSet` event loops.
2. **MMIO & Driver Foundation (v0.8.2):** First-class `ObjectType::Mapping` capabilities for memory-mapped I/O, physical memory grants, and isolated ring-3 driver address-space models.
3. **Userspace Runtime Library `libgaxera` (v0.8.3):** Safe C/Rust syscall abstractions, user-space heap allocators, IPC/WaitSet/Notification stubs, and process entry routines.
4. **Service Discovery & Management (v0.8.4):** Capability service registry inside `init`, name lookup, endpoint handle grants, and boot protocol evolution.
5. **IPC Fast-Path Performance & Benchmarking (v0.8.5):** Hand-optimized assembly syscall trampolines, sub-microsecond `Call`/`Reply` fast paths, and deterministic latency benchmarks.

---

## 2. Milestone Structure & Acceptance Criteria

### Milestone 0.8.1: Interrupt & Notification Architecture
* **Primary Scope:** `InterruptObject`, `Notification` kernel object, IOAPIC programming, IRQ vector allocation, IRQ masking/unmasking/acknowledgement, and `WaitSet` integration.
* **Deliverables:**
  * Define `ObjectType::InterruptObject` and `ObjectType::Notification` in `gaxera-abi` and `kernel-core`.
  * Implement IOAPIC driver (`ioapic.rs`) for hardware IRQ redirection table programming.
  * Capability-bind hardware interrupts to `Notification` objects. When an IRQ fires, the kernel signals the notification, waking any thread blocked on a bound `WaitSet`.
  * Implement IRQ acknowledgement syscalls (`AckInterrupt`).
* **Acceptance Criterion:** QEMU test profile `test-interrupt-notification` confirms PIT/APIC timer and PS/2 keyboard hardware IRQs delivered to user-space `WaitSet` event loops with clean EOI/ack handling.

### Milestone 0.8.2: MMIO & Driver Foundation
* **Primary Scope:** `ObjectType::Mapping`, Physical memory capabilities, MMIO mapping, user-mode driver access, and driver address-space model.
* **Deliverables:**
  * Define `ObjectType::Mapping` capability in `gaxera-abi` and `kernel-core`.
  * Support physical range mapping grants for hardware MMIO windows (e.g. LAPIC, IOAPIC, GPU framebuffers, NVMe BARs).
  * Enforce strict capability rights (`Rights::READ`, `Rights::WRITE`, `Rights::EXECUTE`) and page protection flags (Cache Disable, Write Through) on MMIO mappings.
* **Acceptance Criterion:** QEMU test profile `test-mmio-driver` confirms user-mode driver process mapping an MMIO BAR window and interacting with hardware registers without ring-0 privilege escalation.

### Milestone 0.8.3: Userspace Runtime Library (`libgaxera`) [COMPLETED]
* **Primary Scope:** `libgaxera` runtime, safe syscall wrappers, IPC API, `WaitSet` API, `Notification` API, user heap allocator, and startup routines.
* **Deliverables:**
  * Create `crates/libgaxera` crate providing safe, idiomatic Rust API wrappers for all Gaxera syscalls.
  * Implement user-space heap allocator (`malloc` / `free`) built on `MapMemory` / `UnmapMemory`.
  * Implement standard thread entry (`_start`), TLS initialization, and panic handling for user-space binaries.
* **Acceptance Criterion:** Refactor `init`, `ramfs`, and `script_session` user binaries to consume `libgaxera` directly, replacing raw inline assembly syscalls. (Verified by host unit tests and QEMU integration test `test-userspace-runtime`).

### Milestone 0.8.4: Service Discovery & Management [COMPLETED]
* **Primary Scope:** Service registry, name lookup, endpoint grants, bootstrap protocol, and `init` service manager.
* **Deliverables:**
  * Implement a capability service registry inside `init` (`ServiceRegistry`).
  * User-mode services (e.g. `ramfs`, `console`, `driver`) register named endpoints (`register_service("ramfs", endpoint_handle)`).
  * Client processes request capability grants for named services (`lookup_service("ramfs") -> endpoint_handle`).
* **Acceptance Criterion:** QEMU integration test `test-service-registry` demonstrates dynamic lookup and IPC rendezvous between an unprivileged client process and `ramfs` via `init` service grants. (Verified by host unit tests and QEMU integration test `test-service-registry`).

### Milestone 0.8.5: IPC Performance & Assembly Fast Paths [COMPLETED]
* **Primary Scope:** Assembly syscall trampolines, fast-path `Call`/`Reply`, benchmark framework, and latency report.
* **Deliverables:**
  * Implement x86_64 assembly syscall trampolines (`sysenter`/`sysexit` / `syscall`/`sysretq`) for 64-byte inline register transfers.
  * Optimize thread context switch fast-paths when performing direct IPC rendezvous between sender and receiver.
  * Build QEMU benchmark suite measuring round-trip IPC cycles and latency.
* **Acceptance Criterion:** Latency benchmark confirms sub-microsecond IPC `Call` $\rightarrow$ `Reply` round-trip execution in QEMU. (Verified by host unit tests and QEMU integration test `test-ipc-benchmark`).

---

## 3. Explicit Non-Goals for v0.8
* Full PCI Express bus enumeration / ACPI AML bytecode interpreter (assigned to v0.9 driver framework).
* Full USB 3.0 xHCI driver stack.
* Multi-core SMP IOAPIC vector rebalancing (assigned to v0.9 SMP initiative).
