# Initiative v1.1.0 Roadmap — Ring-3 Memory Foundation

> **Epoch:** v1.1  
> **Status:** Complete for the Ring-3 memory foundation; IRQ architecture and production-service bootstrap are deferred follow-up work
> **Target Release:** v1.1.0  
> **Governing Specification:** `docs/spec/technical_spec.md`
> **Primary Architecture:** `docs/architecture/userspace_runtime.md`
> **Related Draft:** `docs/architecture/irq_delivery.md`
> **Accepted ADRs:** ADR 0037, ADR 0038, ADR 0039

---

## 1. Initiative Charter & Epoch Objectives

Gaxera `v1.0.0` established a capability-secured microkernel baseline with 99 verified host and QEMU integration tests. The v1.1.0 release closes the Ring-3 memory foundation while keeping two larger follow-up domains explicit: hardware IRQ delivery is still a Draft architecture, and production service migration requires a generic process-bootstrap capability contract that is not yet implemented.

Initiative `v1.1.0` delivers the verified memory mechanism required before those later programs:

1. **Research Gate RG-2 (Ring-3 Heap Runtime & Memory Capabilities):**
   * Capability-mediated anonymous memory creation (`Factory` capabilities, `ResourceDomain` physical byte-quota accounting).
   * Fallible Ring-3 heap runtime (`libgaxera::UserspaceAllocator` fixed-chunk arena).
   * Strict anonymous memory contract: zeroed frames, non-executable (W^X compliant), non-DMA.
   * Reference-class object destruction (capability, mapping, in-flight kernel operation refs).

2. **Explicit follow-up boundaries:**
   * RG-1 hardware IRQ delivery remains governed by the Draft `irq_delivery.md` document and is not claimed by v1.1.0.
   * Phase 8 production-service allocator migration remains deferred until process bootstrap authority and lifecycle semantics are accepted.

---

## 2. Research Gates and Milestones

```
Initiative v1.1.0
 ├── Milestone 1.1.1: Architecture & Decision Records (RG-2 current architecture, RG-1 Draft, ADR 0037–0039)
 ├── Milestone 1.1.2: Core Kernel Memory Accounting & Factory Enforcement (ResourceDomain byte quotas, narrow rights)
 ├── Milestone 1.1.3: Reference Classes & Lineage Revocation (Mapping lineage graph, 3-tier cascade revocation)
 ├── Milestone 1.1.4: Fallible Ring-3 Heap Runtime (libgaxera UserspaceAllocator, compensating transactions)
 ├── Milestone 1.1.5: Hardware Interrupt Delivery (DEFERRED; separate initiative)
 ├── Milestone 1.1.6: Production Service Integration (DEFERRED; requires bootstrap contract)
 └── Milestone 1.1.7: Integration & Evidence Closeout (COMPLETED)
```

---

## 3. Preservation Invariants

* **INV-MEM-01 (Zero Kernel Panic on OOM):** Kernel allocations reachable from userspace are fallible. Quota exhaustion returns `SyscallError::ResourceExhausted` without kernel panic.
* **INV-MEM-02 (Transactional Rollback):** Object creation and mapping operations execute as compensating transactions with complete rollback of physical frames, quota charges, object arena slots, and capability handles on failure.
* **INV-SEC-01 (No Ambient Authority):** Object creation requires presenting an explicit `Factory` capability handle. Anonymous memory mappings are strictly non-executable (`Rights::READ | Rights::WRITE`).
* **INV-IRQ-01 (Capability-Scoped IRQs):** Remains a follow-up invariant for RG-1 and is not a v1.1.0 exit criterion.

---

## 4. Verification Matrix

| Milestone | Target Evidence | Test Method |
| --- | --- | --- |
| **M1.1.2** | ResourceDomain byte quota enforcement, Factory type denial, narrow capability rights. | `cargo test -p kernel-core` & QEMU `test-factory-correctness` |
| **M1.1.3** | Creator exit with delegated memory survival, supervisor revocation cascade, mapping lineage cleanup. | QEMU `memory-lifecycle` and `kernel-core` lineage tests |
| **M1.1.4** | Ring-3 `Box`/`Vec` allocation, fixed-chunk heap growth, compensating transaction rollback, OOM recovery. | `cargo test --workspace --locked` & QEMU `ring3-heap` |
| **M1.1.5** | Hardware IRQ LAPIC vector $\rightarrow$ `Notification` signal $\rightarrow$ driver wakeup. | **Deferred:** Draft `irq_delivery.md`; no v1.1.0 evidence |
| **M1.1.6** | Production service allocator migration and untrusted-input audit. | **Deferred:** requires process bootstrap capability contract |
| **M1.1.7** | Locked workspace and QEMU matrix, release documentation, and evidence record. | `cargo xtask test`, `docs/release/v1.1.0.md`, `docs/evidence/v1.1.0_release_evidence.md` |

---

## 5. Addendum: v1.1.0 Scope Correction (2026-08-03)

The v1.1.0 release is complete exclusively for the Ring-3 Memory Foundation. The original IRQ and production-service milestones (including Phase 8) are explicitly deferred and transferred to `docs/roadmap/roadmap_v12.md`. Phase 8 is not silently counted as complete. This deferral boundary is governed by ADR 0039.
