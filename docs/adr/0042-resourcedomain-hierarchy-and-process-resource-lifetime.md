# ADR-0042: ResourceDomain Hierarchy and Process Resource Lifetime

**Status:** Accepted
**Date:** 2026-08-03
**Amends:** ADR-0038

## Context

ADR-0038 established the `ResourceDomain` for physical memory and object accounting. However, in v1.1.0, `ResourceDomain` allocation was largely independent of process lifecycle, leading to ambiguities regarding quota reservation for child processes, charge propagation, and domain destruction. If a process delegates a `MemoryObject` to a supervisor or sibling and then exits, it was undefined whether the original domain was immediately reclaimed (leading to use-after-free or accounting underflow) or kept alive implicitly.

## Decision

We establish `ResourceDomain` as a hierarchical, generational object directly integrated with the Process lifecycle.
1. **Hierarchy**: Every child process receives a dedicated `ResourceDomain`. Its quota (object quota, capability quota, physical memory quota) is strictly reserved and deducted from its parent's `ResourceDomain`.
2. **Charge Propagation**: Resource allocations by the child charge the child domain and are transitively accounted against the parent's reserved quota limit.
3. **Delegated Memory Survival**: If a process exits, its `ResourceDomain` is not destroyed immediately if delegated capabilities (e.g., `MemoryObjects`) still reference it. The domain persists in a detached state.
4. **Destruction Conditions**: A domain is only destroyed when its reference counts for processes, objects, capabilities, physical memory bytes, and child domains all reach zero.
5. **Rollback**: If Process creation fails after domain reservation, the reserved quota is deterministically refunded to the parent domain without leakage.

## Consequences

- **Easier**: Memory delegation is safe; processes can exit without crashing downstream consumers of their delegated memory, while accounting remains mathematically coherent.
- **Easier**: Supervisor limits on child resource consumption are enforced cleanly by the kernel at the capability boundary.
- **Harder**: The kernel's teardown path for `ResourceDomain` requires strict generational reference counting to avoid double-refunds or dangling pointers.

## Alternatives Considered

- **Immediate Quota Refund on Process Exit**: Reclaiming the domain immediately upon process exit. Rejected because it would abruptly invalidate `MemoryObjects` delegated to other processes, violating the capability property that rights only attenuate on delegation and do not asynchronously disappear due to the sender's death.
- **Flat Domain Model**: Giving all processes quotas from a single root pool. Rejected because it breaks process tree encapsulation and prevents `init` from securely capping the memory usage of a sub-process tree (e.g., a driver supervisor limiting its drivers).
