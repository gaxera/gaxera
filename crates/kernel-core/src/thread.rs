use crate::object::ObjectId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadState {
    New,
    Runnable,
    Running,
    Blocked,
    Dying,
    Dead,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadError {
    InvalidTransition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Thread<T> {
    id: ObjectId,
    state: ThreadState,
    address_space: Option<ObjectId>,
    cspace: Option<ObjectId>,
    base_priority: u8,
    effective_priority: u8,
    pub arch: T,
    pub ipc_receive_buffer: Option<gaxera_abi::ipc::InlineMessage>,
}

impl<T> Thread<T> {
    pub fn new(id: ObjectId, address_space: Option<ObjectId>, arch: T) -> Self {
        Self {
            id,
            state: ThreadState::New,
            address_space,
            cspace: None,
            base_priority: 0,
            effective_priority: 0,
            arch,
            ipc_receive_buffer: None,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn state(&self) -> ThreadState {
        self.state
    }

    pub fn address_space(&self) -> Option<ObjectId> {
        self.address_space
    }

    pub fn base_priority(&self) -> u8 {
        self.base_priority
    }

    pub fn effective_priority(&self) -> u8 {
        self.effective_priority
    }

    pub fn set_base_priority(&mut self, priority: u8) {
        self.base_priority = priority;
        self.effective_priority = core::cmp::max(self.effective_priority, priority);
    }

    pub fn boost_priority(&mut self, caller_priority: u8) {
        self.effective_priority = core::cmp::max(self.effective_priority, caller_priority);
    }

    pub fn restore_priority(&mut self) {
        self.effective_priority = self.base_priority;
    }

    pub fn cspace(&self) -> Option<ObjectId> {
        self.cspace
    }

    pub fn set_cspace(&mut self, cspace: ObjectId) {
        self.cspace = Some(cspace);
    }

    pub fn set_aspace(&mut self, aspace: Option<ObjectId>) {
        self.address_space = aspace;
    }

    pub fn make_runnable(&mut self) -> Result<(), ThreadError> {
        match self.state {
            ThreadState::New | ThreadState::Running | ThreadState::Blocked | ThreadState::Dead => {
                self.state = ThreadState::Runnable;
                Ok(())
            }
            _ => Err(ThreadError::InvalidTransition),
        }
    }

    pub fn make_running(&mut self) -> Result<(), ThreadError> {
        match self.state {
            ThreadState::Runnable => {
                self.state = ThreadState::Running;
                Ok(())
            }
            _ => Err(ThreadError::InvalidTransition),
        }
    }

    pub fn make_blocked(&mut self) -> Result<(), ThreadError> {
        match self.state {
            ThreadState::Running => {
                self.state = ThreadState::Blocked;
                Ok(())
            }
            _ => Err(ThreadError::InvalidTransition),
        }
    }

    pub fn make_dying(&mut self) -> Result<(), ThreadError> {
        match self.state {
            ThreadState::Running | ThreadState::Runnable | ThreadState::Blocked => {
                self.state = ThreadState::Dying;
                Ok(())
            }
            _ => Err(ThreadError::InvalidTransition),
        }
    }

    pub fn make_dead(&mut self) -> Result<(), ThreadError> {
        match self.state {
            ThreadState::Dying => {
                self.state = ThreadState::Dead;
                Ok(())
            }
            _ => Err(ThreadError::InvalidTransition),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn thread_priority_inheritance_boost_and_restore() {
        let mut server = Thread::new(test_id(1), None, ());
        server.set_base_priority(10);
        assert_eq!(server.base_priority(), 10);
        assert_eq!(server.effective_priority(), 10);

        // High priority client (priority 50) calls server
        let client_priority = 50;
        server.boost_priority(client_priority);
        assert_eq!(server.base_priority(), 10);
        assert_eq!(server.effective_priority(), 50);

        // Lower priority client (priority 20) calls -> effective remains 50
        server.boost_priority(20);
        assert_eq!(server.effective_priority(), 50);

        // Server replies and restores priority
        server.restore_priority();
        assert_eq!(server.effective_priority(), 10);
    }
}
