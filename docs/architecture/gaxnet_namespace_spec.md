# GaxNet Network Namespace (`NetNamespace`) Architecture

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Object Model](gaxnet_object_model.md), [Capability Model](gaxnet_capability_model.md)

---

## 1. Overview & First-Class Object Status

GaxNet defines **`NetNamespace`** as a first-class, capability-governed network isolation object. Rather than sharing a single global network environment, applications operate within an assigned `NetNamespace`.

```
                            +--------------------------+
                            |     Root NetNamespace    |
                            |   (Default System Scope) |
                            +------------+-------------+
                                         |
               +-------------------------+-------------------------+
               | (Derive Child Namespace)                          | (Derive Isolated Sandbox)
               v                                                   v
+------------------------------+                  +------------------------------+
|     App NetNamespace A       |                  |     App NetNamespace B       |
|  - Interfaces: [eth0]        |                  |  - Interfaces: [veth0]       |
|  - Routes: [10.0.0.0/8]      |                  |  - Routes: [192.168.1.0/24]  |
|  - Resolver: [10.0.0.1]      |                  |  - Resolver: [1.1.1.1]       |
|  - Sessions: [S1, S2]        |                  |  - Sessions: [S3]            |
+------------------------------+                  +------------------------------+
```

---

## 2. Encapsulated Namespace Resources

Each `NetNamespace` object encapsulates its own isolated set of:
- **`NetInterface` Handles:** Virtual or physical network interface bindings.
- **Routing Table (`NetRoute` array):** Isolated IP subnet routes and default gateways.
- **`NetListener` Backlog:** Bound listening ports.
- **Active `NetSession` State:** Communication sessions.
- **Resolver Policy Configuration:** Custom DNS resolution endpoints and search domains.

---

## 3. Namespace Creation, Delegation, & Isolation Invariants

1. **Isolation Invariant:** A task executing inside `NetNamespace B` cannot observe, bind, or send packets over interfaces or routes owned by `NetNamespace A` without holding an explicit cross-namespace `CapabilityHandle`.
2. **Dynamic Creation:** Sub-namespaces are created via `NetNamespace::create_child(&parent_handle, policy_scope)`.
3. **Hierarchy & Cleanup:** Destroying a `NetNamespace` object automatically tears down all encapsulated `NetListener` handles, flushes active `NetSession` capabilities, and reclaims virtual veth interfaces.
