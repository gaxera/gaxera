#[allow(unused_imports)]
use crate::syscall::exit;

/// Validate and borrow the manifest supplied by the kernel at process entry.
///
/// # Safety
/// The caller must pass the exact `RDI`/`RSI` values supplied by the kernel;
/// the pointer must identify a readable manifest in the new process address
/// space for the duration of the returned borrow.
pub unsafe fn bootstrap_manifest(
    pointer: *const gaxera_abi::boot::BootstrapManifest,
    length: usize,
) -> Result<&'static gaxera_abi::boot::BootstrapManifest, gaxera_abi::boot::BootstrapManifestError>
{
    if pointer.is_null()
        || !(pointer as usize)
            .is_multiple_of(core::mem::align_of::<gaxera_abi::boot::BootstrapManifest>())
        || length < core::mem::size_of::<u64>()
    {
        return Err(gaxera_abi::boot::BootstrapManifestError::BadLength);
    }
    // SAFETY: The caller's safety contract establishes that this range is a
    // readable kernel-created manifest in the current process address space.
    let manifest = unsafe { &*pointer };
    let serialized_size = usize::try_from(manifest.total_size)
        .map_err(|_| gaxera_abi::boot::BootstrapManifestError::BadLength)?;
    if serialized_size < usize::from(gaxera_abi::boot::BootstrapManifest::HEADER_SIZE)
        || serialized_size > length
    {
        return Err(gaxera_abi::boot::BootstrapManifestError::BadLength);
    }
    manifest.validate()?;
    Ok(manifest)
}

/// Return an opaque capability from a validated manifest by role and ordinal.
/// The caller never depends on the slot number chosen by the kernel.
pub fn manifest_capability(
    manifest: &gaxera_abi::boot::BootstrapManifest,
    role: gaxera_abi::boot::BootstrapRole,
    ordinal: usize,
) -> Option<gaxera_abi::Handle> {
    let mut seen = 0usize;
    for entry in &manifest.entries[..usize::from(manifest.entry_count)] {
        if entry.role == role as u16 {
            if seen == ordinal {
                return Some(entry.handle);
            }
            seen += 1;
        }
    }
    None
}

/// Validate a kernel entry manifest and initialize a userspace allocator from
/// its explicit HeapFactory and SelfAddressSpace capabilities.
///
/// # Safety
/// The caller must pass the exact `pointer` and `length` supplied by the kernel;
/// `pointer` must identify a valid, readable `BootstrapManifest` in the current address space.
pub unsafe fn initialize_userspace_allocator(
    pointer: *const gaxera_abi::boot::BootstrapManifest,
    length: usize,
    allocator: &crate::allocator::UserspaceAllocator,
) -> Result<&'static gaxera_abi::boot::BootstrapManifest, gaxera_abi::boot::BootstrapManifestError>
{
    // SAFETY: The caller is the process entry point and supplies the kernel's
    // validated RDI/RSI manifest pair.
    let manifest = unsafe { bootstrap_manifest(pointer, length) }?;
    let factory = manifest_capability(manifest, gaxera_abi::boot::BootstrapRole::HeapFactory, 0)
        .ok_or(gaxera_abi::boot::BootstrapManifestError::InvalidHandle { index: 0 })?;
    let aspace = manifest_capability(
        manifest,
        gaxera_abi::boot::BootstrapRole::SelfAddressSpace,
        0,
    )
    .ok_or(gaxera_abi::boot::BootstrapManifestError::InvalidHandle { index: 0 })?;
    allocator.init(factory, aspace);
    Ok(manifest)
}

/// Standard ring-3 process entrypoint `_start`.
///
/// # Safety
/// Executed by the kernel on ring-3 process startup.
#[cfg(all(feature = "entry", not(test)))]
#[no_mangle]
pub unsafe extern "C" fn _start(
    manifest_pointer: *const gaxera_abi::boot::BootstrapManifest,
    manifest_length: usize,
) -> ! {
    // SAFETY: The kernel supplies RDI/RSI with the validated manifest range.
    if unsafe { bootstrap_manifest(manifest_pointer, manifest_length) }.is_err() {
        exit(gaxera_abi::status::INVALID_ARGUMENT);
    }
    extern "Rust" {
        fn main() -> i32;
    }

    let status = unsafe { main() };
    exit(status as u64);
}
