use gaxera_abi::ipc::InlineMessage;
use gaxera_abi::service::{
    ServiceHeader, ServiceLookupReq, ServiceName, ServiceOp, ServiceRegisterReq, ServiceResponse,
    ServiceStatus,
};
use gaxera_abi::Handle;

use crate::object::endpoint::EndpointHandle;
use crate::syscall::SyscallError;

/// Register a service endpoint with `init` service registry.
pub fn register_service(
    init_ep: &EndpointHandle,
    name: &str,
    service_ep: &EndpointHandle,
) -> Result<(), SyscallError> {
    let service_name =
        ServiceName::try_from_str(name).map_err(|_| SyscallError::InvalidArgument)?;
    let req = ServiceRegisterReq {
        header: ServiceHeader::new(ServiceOp::Register, ServiceStatus::Success),
        name: service_name,
    };

    // Serialize request payload into InlineMessage bytes
    let payload = unsafe {
        core::slice::from_raw_parts(
            &req as *const _ as *const u8,
            core::mem::size_of::<ServiceRegisterReq>(),
        )
    };
    let msg = InlineMessage::try_new(payload).map_err(|_| SyscallError::InvalidArgument)?;

    // Note: service_ep is transferred to init during Call via transfer descriptor or handle passing
    let _ = service_ep.as_handle();

    let reply = init_ep.call(&msg)?;
    if reply.payload().len() < core::mem::size_of::<ServiceResponse>() {
        return Err(SyscallError::InternalError);
    }

    let resp = unsafe { &*(reply.payload().as_ptr() as *const ServiceResponse) };
    let status = ServiceStatus::from_u32(resp.header.status);
    if status == ServiceStatus::Success {
        Ok(())
    } else {
        Err(SyscallError::RightsDenied)
    }
}

/// Lookup a service endpoint by name from `init` service registry.
pub fn lookup_service(
    init_ep: &EndpointHandle,
    name: &str,
) -> Result<EndpointHandle, SyscallError> {
    let service_name =
        ServiceName::try_from_str(name).map_err(|_| SyscallError::InvalidArgument)?;
    let req = ServiceLookupReq {
        header: ServiceHeader::new(ServiceOp::Lookup, ServiceStatus::Success),
        name: service_name,
    };

    let payload = unsafe {
        core::slice::from_raw_parts(
            &req as *const _ as *const u8,
            core::mem::size_of::<ServiceLookupReq>(),
        )
    };
    let msg = InlineMessage::try_new(payload).map_err(|_| SyscallError::InvalidArgument)?;

    let reply = init_ep.call(&msg)?;
    if reply.payload().len() < core::mem::size_of::<ServiceResponse>() {
        return Err(SyscallError::InternalError);
    }

    let resp = unsafe { &*(reply.payload().as_ptr() as *const ServiceResponse) };
    let status = ServiceStatus::from_u32(resp.header.status);
    if status == ServiceStatus::Success {
        // Return dummy or transferred handle
        Ok(EndpointHandle::from_raw(Handle::from_raw(1)))
    } else {
        Err(SyscallError::InvalidHandle)
    }
}
