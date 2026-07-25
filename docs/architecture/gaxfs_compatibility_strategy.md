# GaxFS Compatibility Strategy Specification

> **Status:** Authoritative Architecture Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `libgaxera::vfs`, `vfs_server`  

---

## 1. Principle of Isolated & One-Way Compatibility

Native GaxFS primitives (128-bit `GaxObjectId`, capability handles, metadata dictionaries, relationship graph edges) must **never be compromised or polluted** to support legacy APIs.

Legacy compatibility is provided entirely via isolated Ring-3 translation wrappers:

```
[Legacy POSIX / Windows / WASI Application]
                    │ (open, read, write, stat, close)
                    ▼
[CompatibilityProvider] (Ring-3 Semantic Translation Wrappers)
                    │
                    ▼
[libgaxera::vfs]        (Native VFS Client Adapter)
                    │
                    ▼
[vfs_server]            (Virtual File System & Namespace Router)
                    │
                    ▼
[gaxfs_server]          (GaxFS Object Engine)
```

---

## 2. Generic `CompatibilityProvider` Architecture

Legacy API adaptation is abstracted behind a generic Ring-3 contract: **`CompatibilityProvider`**.

```rust
pub trait CompatibilityProvider {
    /// Translates a legacy path string relative to a capability scope handle
    fn open(&self, scope: &CapabilityHandle, path: &str, flags: u32) -> Result<LegacyHandle, LegacyError>;
    
    /// Executes payload reads through native capability handle streams
    fn read(&self, handle: LegacyHandle, buf: &mut [u8]) -> Result<usize, LegacyError>;
    
    /// Executes payload writes through native Copy-on-Write storage
    fn write(&self, handle: LegacyHandle, buf: &[u8]) -> Result<usize, LegacyError>;
    
    /// Translates native GaxObject metadata into legacy stat structures
    fn stat(&self, handle: LegacyHandle) -> Result<LegacyStat, LegacyError>;
    
    /// Releases local compatibility handle mappings
    fn close(&self, handle: LegacyHandle) -> Result<(), LegacyError>;
}
```

### Provider Implementations:
- **`PosixCompatibilityProvider`:** Translates POSIX `open`, `read`, `write`, `stat`, `close` calls.
- **`Win32CompatibilityProvider`:** Translates Win32 `CreateFileA`, `ReadFile`, `WriteFile` handles.
- **`MacOsCompatibilityProvider`:** Maps macOS extended attributes (`xattr`) to native metadata keys.
- **Pluggable Future Providers:** `WasiCompatibilityProvider`, Linux emulation layers, language runtime adapters, legacy SDKs.

---

## 3. Core Architectural Invariants

> **Invariant 1 — One-Way Compatibility:**  
> Compatibility is strictly **one-way**. Legacy abstractions are translated into native GaxFS primitives. Native architectural decisions are NEVER constrained by legacy API behavior.
> 
> **Invariant 2 — Native Architecture Authority:**  
> Native GaxFS semantics remain authoritative. Compatibility layers translate legacy concepts into native primitives without redefining or weakening capability security, object identity (`GaxObjectId`), namespaces, metadata dictionaries, or the storage model.
> 
> **Invariant 3 — Capability Scope Preservation:**  
> Legacy path resolution always executes relative to an authorized `CapabilityHandle` scope. Compatibility providers **must never** introduce ambient global path authority.

---

## 4. Semantic Translation & Handle Mapping

1. **Semantic Translation over Exact Emulation:** Compatibility providers perform semantic translation, exposing the closest equivalent behavior supported by native GaxFS. Legacy semantics are not exact-reproduced when they fundamentally conflict with capability-based security.
2. **Handle Abstraction:** Legacy integer file descriptors and Win32 handles are thread-local compatibility indices mapped to native `CapabilityHandle` abstractions. Legacy applications never obtain direct access to native capability internals.
3. **Legacy Metadata Mapping:** Legacy metadata (POSIX mode bits, extended attributes) is represented through native metadata dictionary attributes (`attributes["posix_mode"] = "0755"`). Provider-specific metadata remains isolated.
4. **Error Convention Translation:** Compatibility providers translate native filesystem errors (`VfsError`) into expected legacy API conventions (POSIX `errno` values, Win32 error codes). Native GaxFS remains completely unaware of legacy conventions.
