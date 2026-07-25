# GaxFS Namespace & View Architecture Specification

> **Status:** Authoritative Architecture Document  
> **Date:** 2026-07-25  
> **Initiative:** Milestone 0.9.3 — GaxFS Native Storage Platform (`docs/roadmap/roadmap_v09.md`)  
> **Applies To:** `vfs_server`, `gaxfs_server`, `libgaxera::vfs`  

---

## 1. First-Principles Namespace Architecture

In legacy operating systems, the filesystem hierarchy is bound to a single, rigid string path tree (`/usr/bin/gcc`, `C:\Windows\System32`).

GaxFS decouples **Object Identity** (`GaxObjectId`) from **Namespaces**.
- `GaxObjectId` is the permanent, location-independent handle to stored data.
- **Namespaces are projections** that map organizational structures or dynamic query expressions onto underlying `GaxObjectId`s.
- **Storage Decoupling:** Namespace views organize objects but **never own, copy, or duplicate object storage**. Objects exist independently of every namespace.

---

## 2. Extensible `NamespaceProvider` Architecture

Every namespace view in GaxFS implements a common architectural contract: **`NamespaceProvider`**.

```rust
pub trait NamespaceProvider {
    /// Resolves a path expression within the provider scope to an object handle
    fn resolve(&self, scope_handle: &CapabilityHandle, path: &str) -> Result<(GaxObjectId, CapabilityHandle), VfsError>;
    
    /// Enumerates children or items contained in the namespace node
    fn enumerate(&self, scope_handle: &CapabilityHandle) -> Result<Vec<NamespaceEntry>, VfsError>;
    
    /// Direct lookup of a single namespace entry
    fn lookup(&self, scope_handle: &CapabilityHandle, name: &str) -> Result<NamespaceEntry, VfsError>;
    
    /// Registers a change watcher on the namespace provider
    fn watch(&self, scope_handle: &CapabilityHandle, subscriber: EventSubscriber) -> Result<WatchHandle, VfsError>;
}
```

---

## 3. Pluggable Namespace Views

GaxFS natively supports multiple concurrent namespace views over the same underlying object store:

```
                            [GaxFS Object Store]
                                     │
      ┌───────────────┬──────────────┼──────────────┬───────────────┬──────────────┐
      ▼               ▼              ▼              ▼               ▼              ▼
[DirectoryView] [ProjectView]    [TagView]   [WorkspaceView] [DynamicCollection] [CustomView]
```

### 3.1 Namespace View Definitions
1. **Traditional Directory View (`DirectoryView`):** Standard hierarchical path mapping (`/projects/gaxera/src/main.rs`).
2. **Project View (`ProjectView`):** Projections grouping objects by project metadata attributes (`project == "gaxera"`).
3. **Tag View (`TagView`):** Categorical views organized by metadata tags (`/tags/kernel/v0.9.3`).
4. **Workspace View (`WorkspaceView`):** Projections filtering objects by active developer session or workspace task metadata.
5. **Dynamic Collection View (`DynamicCollectionView`):** Dynamic query views produced by `CollectionProvider` implementations (e.g. "Design docs modified in the last 48 hours referencing scheduling").
6. **Custom Application View (`AppCustomView`):** Application-defined projection schemas mapping domain models to virtual filesystem nodes.
7. **Extensible Future Views:** Pluggable providers including `GitView`, `CalendarView`, `PackageView`, and `MediaView`.

### 3.2 Metadata Ownership & Separation
`ProjectView` and `WorkspaceView` are projections over metadata supplied by higher-level services or client applications. **GaxFS itself owns zero project or workspace business logic**—it simply evaluates namespace projections over object metadata attributes.

### 3.3 Generic `CollectionProvider` Architecture
Dynamic collection views are produced by a generic **`CollectionProvider`** interface. `knowledge_server` is one possible implementation; other producers include `search_server`, IDE tools, Git integrations, package managers, and application plugins.

---

## 4. Core Namespace Invariants

> **Invariant 1 — Namespace Independence:**  
> Namespace operations (`rename`, `move`, `retag`, workspace reorganization, collection recomputation) **NEVER alter `GaxObjectId`**. Only namespace view mappings are updated.
> 
> **Invariant 2 — Multiple Namespace Membership:**  
> A single object may exist simultaneously across multiple namespace views (`DirectoryView`, `TagView`, `ProjectView`, `DynamicCollectionView`). Membership in one namespace does not implicitly modify membership in another.

---

## 5. Path Resolution & Capability-Mediated Access

### 5.1 Path Resolution Protocol
Path resolution in `vfs_server` is executed relative to an open directory capability handle:

$$\text{resolve\_path}(\text{start\_dir\_handle}, \text{"src/main.rs"}) \longrightarrow \text{Result}<(\text{GaxObjectId}, \text{CapabilityHandle}), \text{VfsError}>$$

### 5.2 Security Invariant: Zero Ambient Path Access
- Knowing a path string (`/etc/passwd` or `/home/user/secret.pdf`) grants **zero access**.
- To resolve a path, the requesting process **must present a valid capability handle** to the starting directory node.
- Path traversal (`../../etc/passwd`) past the boundary of the starting directory handle is strictly blocked by `vfs_server` with `AccessDenied`.
