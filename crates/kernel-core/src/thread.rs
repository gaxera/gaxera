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
    pub arch: T,
}

impl<T> Thread<T> {
    pub fn new(id: ObjectId, address_space: Option<ObjectId>, arch: T) -> Self {
        Self {
            id,
            state: ThreadState::New,
            address_space,
            arch,
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

    pub fn make_runnable(&mut self) -> Result<(), ThreadError> {
        match self.state {
            ThreadState::New | ThreadState::Running | ThreadState::Blocked => {
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
