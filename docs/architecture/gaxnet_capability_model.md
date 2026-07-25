# GaxNet Capability Delegation & Security Architecture

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [Object Model](gaxnet_object_model.md), [Design Principles](gaxnet_design_principles.md)

---

## 1. Capability Delegation & Attenuation Pipeline

GaxNet capability propagation follows strict architectural invariants across task hierarchies:

```
+-------------------------------------------------------------------+
|                        Supervisor Process                         |
|   (Holds Root Network Capability: Full NetRights & Unscoped Scope)    |
+-------------------------------------------------------------------+
                                  │
                                  ▼ (Derive Scoped Capability)
+-------------------------------------------------------------------+
|                         Browser Process                           |
|   (Rights: NET_CONNECT | NET_RESOLVE | Scope: *.gaxera.org:443)     |
+-------------------------------------------------------------------+
                                  │
                                  ▼ (Derive Attenuated Capability)
+-------------------------------------------------------------------+
|                      HTTPS-only Tab Manager                       |
|   (Rights: NET_CONNECT | Scope: api.gaxera.org:443 | Port: 443)    |
+-------------------------------------------------------------------+
                                  │
                                  ▼ (Derive Read-only Session)
+-------------------------------------------------------------------+
|                         Renderer Process                          |
|   (Rights: NET_READ | Scope: Established NetSession Handle Only)  |
+-------------------------------------------------------------------+
```

---

## 2. Delegation, Attenuation, & Lifetime Invariants

1. **Monotonic Rights Attenuation:** A child task cannot hold rights that the parent task lacks. Calling `derive_narrowed` can only remove rights or narrow policy constraints.
2. **Policy Scope Narrowing:** CIDR blocks (e.g. `10.0.0.0/8` $\rightarrow$ `10.1.2.0/24`), domain globs (`*.org` $\rightarrow$ `api.org`), and port ranges (`1..65535` $\rightarrow$ `443..443`) can only be narrowed.
3. **Instantaneous Tree Revocation:** Revoking a parent capability in the capability tree invalidates all descendant capability handles across all task address spaces instantly.
4. **Ownership & Inheritance:** Child tasks inherit capability handles explicitly via IPC capability transfer tokens, never implicitly via ambient environment variables or process cloning.
5. **Lifetime Propagation:** A derived network handle cannot outlive the parent session or capability lease timestamp.

---

## 3. Dedicated `NetRights` Bitfield

```rust
pub struct NetRights(u64);

impl NetRights {
    pub const READ: Self        = Self(1 << 0);  // Read incoming payload packets
    pub const WRITE: Self       = Self(1 << 1);  // Write outgoing payload packets
    pub const CONTROL: Self     = Self(1 << 2);  // Modify session options / close
    pub const CONNECT: Self     = Self(1 << 16); // Outbound session creation
    pub const LISTEN: Self      = Self(1 << 17); // Inbound server port binding
    pub const BIND_RAW: Self    = Self(1 << 18); // Low-port (<1024) binding
    pub const RESOLVE: Self     = Self(1 << 19); // Domain name lookup via DNS
    pub const MULTICAST: Self   = Self(1 << 20); // IGMP / Multicast group join
    pub const LAN_ONLY: Self    = Self(1 << 21); // Restrict to RFC 1918 private subnets
    pub const PROMISCUOUS: Self = Self(1 << 22); // Raw frame capture (Admin only)
}
```
