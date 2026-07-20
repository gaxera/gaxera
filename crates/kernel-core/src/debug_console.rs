use crate::object::ObjectId;

/// A capability representing a debugging output console.
///
/// This provides a secure way for userspace to print messages to the
/// host serial port or screen without needing privileged port I/O access.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DebugConsole {
    id: ObjectId,
}

impl DebugConsole {
    pub fn new(id: ObjectId) -> Self {
        Self { id }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }
}
