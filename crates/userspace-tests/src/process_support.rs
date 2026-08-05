use gaxera_abi::boot::{BootstrapManifest, BootstrapRole};
use gaxera_abi::{Handle, ProcessControlOp};
use libgaxera::syscall;

/// Exercise the process object through the public ABI without attempting to
/// start an unbacked child image. Image execution is tested separately once a
/// loader-provisioned executable is supplied to the child.
pub fn create_terminate_reap(factory: Handle, status: u64) -> Result<(), ()> {
    let process = syscall::create_process(factory, 128, 128, 4 * 1024 * 1024).map_err(|_| ())?;
    let state =
        syscall::process_control(process, ProcessControlOp::Query, 0, 0, 0).map_err(|_| ())?;
    if state != 1 {
        return Err(());
    }
    let aspace = syscall::process_control(process, ProcessControlOp::AcquireAddressSpace, 0, 0, 0)
        .map(Handle::from_raw)
        .map_err(|_| ())?;
    let thread = syscall::process_control(process, ProcessControlOp::AcquireMainThread, 0, 0, 0)
        .map(Handle::from_raw)
        .map_err(|_| ())?;
    syscall::delete_handle(aspace).map_err(|_| ())?;
    syscall::delete_handle(thread).map_err(|_| ())?;
    syscall::process_control(process, ProcessControlOp::Terminate, status, 0, 0).map_err(|_| ())?;
    let state =
        syscall::process_control(process, ProcessControlOp::Query, 0, 0, 0).map_err(|_| ())?;
    if state != 6 {
        return Err(());
    }
    syscall::process_control(process, ProcessControlOp::Reap, 0, 0, 0).map_err(|_| ())?;
    Ok(())
}

pub fn capability(manifest: &BootstrapManifest, role: BootstrapRole) -> Option<Handle> {
    manifest.entries[..usize::from(manifest.entry_count)]
        .iter()
        .find(|entry| entry.role == role as u16)
        .map(|entry| entry.handle)
}
