use crate::object::ObjectId;
use crate::resource::ResourceDomainId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessState {
    New,
    Prepared,
    Runnable,
    Running,
    ExitRequested,
    Exiting,
    Zombie,
    Reaped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessError {
    InvalidStateTransition,
    MissingComponents,
    InvalidConfiguration,
}

pub struct Process {
    id: ObjectId,
    state: ProcessState,
    domain: ResourceDomainId,
    address_space: Option<ObjectId>,
    capability_space: Option<ObjectId>,
    main_thread: Option<ObjectId>,
    supervisor: Option<ObjectId>,
    exit_notification: Option<ObjectId>,
    exit_status: Option<u64>,
    main_thread_configured: bool,
    bootstrap_nodes: [Option<ObjectId>; 32],
    bootstrap_manifest_frame: Option<u64>,
    bootstrap_manifest_vaddr: Option<u64>,
    bootstrap_manifest_size: Option<u32>,
    bootstrap_factory: Option<ObjectId>,
    installed_roles: [Option<u16>; 32],
}

impl Process {
    pub fn new(id: ObjectId, domain: ResourceDomainId) -> Self {
        Self {
            id,
            state: ProcessState::New,
            domain,
            address_space: None,
            capability_space: None,
            main_thread: None,
            supervisor: None,
            exit_notification: None,
            exit_status: None,
            main_thread_configured: false,
            bootstrap_nodes: [None; 32],
            bootstrap_manifest_frame: None,
            bootstrap_manifest_vaddr: None,
            bootstrap_manifest_size: None,
            bootstrap_factory: None,
            installed_roles: [None; 32],
        }
    }

    pub fn state(&self) -> ProcessState {
        self.state
    }

    pub const fn id(&self) -> ObjectId {
        self.id
    }

    pub fn domain(&self) -> ResourceDomainId {
        self.domain
    }

    pub const fn address_space(&self) -> Option<ObjectId> {
        self.address_space
    }

    pub const fn capability_space(&self) -> Option<ObjectId> {
        self.capability_space
    }

    pub const fn main_thread(&self) -> Option<ObjectId> {
        self.main_thread
    }

    pub const fn supervisor(&self) -> Option<ObjectId> {
        self.supervisor
    }

    pub const fn exit_notification(&self) -> Option<ObjectId> {
        self.exit_notification
    }

    pub const fn exit_status(&self) -> Option<u64> {
        self.exit_status
    }

    pub const fn main_thread_configured(&self) -> bool {
        self.main_thread_configured
    }

    // Component bindings
    pub fn bind_address_space(&mut self, id: ObjectId) -> Result<(), ProcessError> {
        if self.state != ProcessState::New {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.address_space = Some(id);
        Ok(())
    }

    pub fn bind_capability_space(&mut self, id: ObjectId) -> Result<(), ProcessError> {
        if self.state != ProcessState::New {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.capability_space = Some(id);
        Ok(())
    }

    pub fn bind_main_thread(&mut self, id: ObjectId) -> Result<(), ProcessError> {
        if self.state != ProcessState::New {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.main_thread = Some(id);
        Ok(())
    }

    pub fn bind_supervisor(&mut self, id: ObjectId) -> Result<(), ProcessError> {
        if self.state != ProcessState::New {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.supervisor = Some(id);
        Ok(())
    }

    pub fn bind_exit_notification(&mut self, id: ObjectId) -> Result<(), ProcessError> {
        if self.state != ProcessState::New {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.exit_notification = Some(id);
        Ok(())
    }

    pub fn bind_bootstrap_node(&mut self, index: usize, id: ObjectId) -> Result<(), ProcessError> {
        if self.state != ProcessState::New {
            return Err(ProcessError::InvalidStateTransition);
        }
        if index >= 32 {
            return Err(ProcessError::InvalidConfiguration);
        }
        self.bootstrap_nodes[index] = Some(id);
        Ok(())
    }

    /// Records that the main thread has a validated user entry context and
    /// belongs to this process's address and capability spaces.
    pub fn configure_main_thread(&mut self) -> Result<(), ProcessError> {
        if self.state != ProcessState::Prepared || self.main_thread.is_none() {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.main_thread_configured = true;
        Ok(())
    }

    pub fn bootstrap_nodes(&self) -> &[Option<ObjectId>; 32] {
        &self.bootstrap_nodes
    }

    pub const fn bootstrap_manifest(&self) -> Option<(u64, u64, u32)> {
        match (
            self.bootstrap_manifest_frame,
            self.bootstrap_manifest_vaddr,
            self.bootstrap_manifest_size,
        ) {
            (Some(frame), Some(vaddr), Some(size)) => Some((frame, vaddr, size)),
            _ => None,
        }
    }

    pub fn update_bootstrap_manifest_size(&mut self, size: u32) -> Result<(), ProcessError> {
        if self.bootstrap_manifest_frame.is_none() || self.bootstrap_manifest_vaddr.is_none() {
            return Err(ProcessError::InvalidConfiguration);
        }
        self.bootstrap_manifest_size = Some(size);
        Ok(())
    }

    pub fn bind_bootstrap_manifest(
        &mut self,
        frame: u64,
        vaddr: u64,
        size: u32,
    ) -> Result<(), ProcessError> {
        if self.state != ProcessState::New || size == 0 {
            return Err(ProcessError::InvalidConfiguration);
        }
        if self.bootstrap_manifest_frame.is_some() {
            return Err(ProcessError::InvalidConfiguration);
        }
        self.bootstrap_manifest_frame = Some(frame);
        self.bootstrap_manifest_vaddr = Some(vaddr);
        self.bootstrap_manifest_size = Some(size);
        Ok(())
    }

    pub fn bind_bootstrap_factory(&mut self, id: ObjectId) -> Result<(), ProcessError> {
        if self.state != ProcessState::New || self.bootstrap_factory.is_some() {
            return Err(ProcessError::InvalidConfiguration);
        }
        self.bootstrap_factory = Some(id);
        Ok(())
    }

    pub const fn bootstrap_factory(&self) -> Option<ObjectId> {
        self.bootstrap_factory
    }

    pub fn record_installed_role(&mut self, role: u16) -> Result<(), ProcessError> {
        // Device mappings, boot modules, and service endpoints are repeatable
        // manifest roles. Singleton roles retain their role-indexed duplicate
        // protection.
        let repeatable = matches!(role, 5 | 8 | 9);
        if repeatable {
            let slot = self
                .installed_roles
                .iter()
                .position(Option::is_none)
                .ok_or(ProcessError::InvalidConfiguration)?;
            self.installed_roles[slot] = Some(role);
            return Ok(());
        }

        let slot = usize::from(role);
        if slot >= self.installed_roles.len() || self.installed_roles[slot].is_some() {
            return Err(ProcessError::InvalidConfiguration);
        }
        self.installed_roles[slot] = Some(role);
        Ok(())
    }

    // State transitions
    pub fn prepare(&mut self) -> Result<(), ProcessError> {
        if self.state != ProcessState::New {
            return Err(ProcessError::InvalidStateTransition);
        }
        if self.address_space.is_none()
            || self.capability_space.is_none()
            || self.main_thread.is_none()
        {
            return Err(ProcessError::MissingComponents);
        }
        self.state = ProcessState::Prepared;
        Ok(())
    }

    pub fn make_runnable(&mut self) -> Result<(), ProcessError> {
        if self.state != ProcessState::Prepared {
            return Err(ProcessError::InvalidStateTransition);
        }
        if !self.main_thread_configured {
            return Err(ProcessError::InvalidConfiguration);
        }
        self.state = ProcessState::Runnable;
        Ok(())
    }

    pub fn make_running(&mut self) -> Result<(), ProcessError> {
        if self.state != ProcessState::Runnable {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.state = ProcessState::Running;
        Ok(())
    }

    pub fn request_exit(&mut self, status: u64) -> Result<(), ProcessError> {
        if self.state == ProcessState::Reaped
            || self.state == ProcessState::Zombie
            || self.state == ProcessState::Exiting
            || self.state == ProcessState::ExitRequested
        {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.exit_status = Some(status);
        self.state = ProcessState::ExitRequested;
        Ok(())
    }

    pub fn mark_exiting(&mut self) -> Result<(), ProcessError> {
        if self.state != ProcessState::ExitRequested {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.state = ProcessState::Exiting;
        Ok(())
    }

    pub fn mark_zombie(&mut self) -> Result<(), ProcessError> {
        if self.state != ProcessState::Exiting {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.state = ProcessState::Zombie;
        Ok(())
    }

    pub fn reap(&mut self) -> Result<(), ProcessError> {
        if self.state != ProcessState::Zombie {
            return Err(ProcessError::InvalidStateTransition);
        }
        self.state = ProcessState::Reaped;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::ObjectId;

    #[test]
    fn test_process_state_transitions() {
        let mut p = Process::new(
            ObjectId::new_for_test(1, 1),
            ResourceDomainId::new_for_test(1),
        );
        assert_eq!(p.state(), ProcessState::New);

        // Invalid transition before binding
        assert_eq!(p.prepare(), Err(ProcessError::MissingComponents));

        let aspace = ObjectId::new_for_test(2, 1);
        let cspace = ObjectId::new_for_test(3, 1);
        let thread = ObjectId::new_for_test(4, 1);

        assert_eq!(p.bind_address_space(aspace), Ok(()));
        assert_eq!(p.bind_capability_space(cspace), Ok(()));
        assert_eq!(p.bind_main_thread(thread), Ok(()));

        assert_eq!(p.prepare(), Ok(()));
        assert_eq!(p.state(), ProcessState::Prepared);

        assert_eq!(p.configure_main_thread(), Ok(()));
        assert_eq!(p.make_runnable(), Ok(()));
        assert_eq!(p.state(), ProcessState::Runnable);

        assert_eq!(p.make_running(), Ok(()));
        assert_eq!(p.state(), ProcessState::Running);

        assert_eq!(p.request_exit(0), Ok(()));
        assert_eq!(p.state(), ProcessState::ExitRequested);

        assert_eq!(p.mark_exiting(), Ok(()));
        assert_eq!(p.state(), ProcessState::Exiting);

        assert_eq!(p.mark_zombie(), Ok(()));
        assert_eq!(p.state(), ProcessState::Zombie);

        assert_eq!(p.reap(), Ok(()));
        assert_eq!(p.state(), ProcessState::Reaped);
    }

    #[test]
    fn test_invalid_transitions() {
        let mut p = Process::new(
            ObjectId::new_for_test(1, 1),
            ResourceDomainId::new_for_test(1),
        );
        assert_eq!(p.make_runnable(), Err(ProcessError::InvalidStateTransition));
        assert_eq!(p.make_running(), Err(ProcessError::InvalidStateTransition));
        assert_eq!(p.request_exit(0), Ok(()));
        assert_eq!(p.make_running(), Err(ProcessError::InvalidStateTransition));
    }
}
