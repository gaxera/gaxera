# GaxNet State Persistence & Identity Storage Format

> **Status:** Canonical | **Milestone Target:** v0.9.4 | **Version:** 1.0  
> **Related Documents:** [ADR 0035](../adr/0035-gaxnet-first-principles-architecture.md), [GaxFS Architecture](gaxfs_specification.md)

---

## 1. Integration with GaxFS Storage Platform

GaxNet leverages the **GaxFS Native Storage Platform** (`gax_storage_engine`) for persistent state storage, certificate identity management, and network session caching.

```
+-----------------------------------------------------------------------+
|                    GaxFS Native Object Storage                        |
+-----------------------------------------------------------------------+
|                                                                       |
|  +--------------------+  +-------------------+  +------------------+  |
|  | Network Config     |  | TLS Certificates  |  | Domain Provenance|  |
|  | Object             |  | & Identity Keys   |  | Record Store     |  |
|  +--------------------+  +-------------------+  +------------------+  |
|                                                                       |
+-----------------------------------------------------------------------+
```

---

## 2. On-Disk Binary Data Formats

### 2.1 Network Interface Configuration Format (`GaxNetConfigHeader`)
```rust
#[repr(C)]
pub struct GaxNetConfigHeader {
    pub magic: [u8; 8],        // b"GAXNETCF"
    pub version: u32,
    pub flags: u32,
    pub mac_address: [u8; 6],
    pub reserved: [u8; 2],
    pub ipv4_address: [u8; 4],
    pub subnet_mask: [u8; 4],
    pub default_gateway: [u8; 4],
    pub dns_servers: [[u8; 4]; 2],
}
```

### 2.2 Domain Provenance Record Format (`GaxNetDnsRecord`)
```rust
#[repr(C)]
pub struct GaxNetDnsRecord {
    pub record_type: u16,
    pub ttl_seconds: u32,
    pub domain_name_len: u16,
    pub ip_address: [u8; 16],
    pub signature_blake3: [u8; 32],
}
```

---

## 3. Crash Consistency & Security Invariants

1. **Transactional Commits:** Network configurations participate in GaxFS Copy-on-Write dual-superblock commits.
2. **Encrypted Identity Keys:** TLS identity certificates and private keys stored in GaxFS are capability-gated, accessible exclusively by authorized network services.
