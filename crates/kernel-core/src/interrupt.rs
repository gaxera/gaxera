use crate::object::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InterruptError {
    InvalidState,
    NotBound,
    Closed,
}

/// First-Class Interrupt Capability (`ObjectType::InterruptObject = 7`).
///
/// Represents exclusive capability authority over a specific hardware IRQ line.
pub struct InterruptObject {
    id: ObjectId,
    vector: u8,
    irq: u8,
    bound_notification: Option<ObjectId>,
    masked: bool,
    closed: bool,
}

impl InterruptObject {
    pub fn new(id: ObjectId, vector: u8, irq: u8) -> Self {
        Self {
            id,
            vector,
            irq,
            bound_notification: None,
            masked: true,
            closed: false,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn vector(&self) -> u8 {
        self.vector
    }

    pub fn irq(&self) -> u8 {
        self.irq
    }

    pub fn is_masked(&self) -> bool {
        self.masked
    }

    pub fn bound_notification(&self) -> Option<ObjectId> {
        self.bound_notification
    }

    pub fn bind_notification(&mut self, notification: ObjectId) -> Result<(), InterruptError> {
        if self.closed {
            return Err(InterruptError::Closed);
        }
        self.bound_notification = Some(notification);
        Ok(())
    }

    pub fn mask(&mut self) {
        self.masked = true;
    }

    pub fn unmask(&mut self) {
        self.masked = false;
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.masked = true;
        self.bound_notification = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn interrupt_object_binding_and_masking() {
        let mut irq_obj = InterruptObject::new(test_id(1), 33, 1);
        let notif_id = test_id(10);

        assert!(irq_obj.is_masked());
        assert_eq!(irq_obj.bound_notification(), None);

        assert_eq!(irq_obj.bind_notification(notif_id), Ok(()));
        assert_eq!(irq_obj.bound_notification(), Some(notif_id));

        irq_obj.unmask();
        assert!(!irq_obj.is_masked());

        irq_obj.mask();
        assert!(irq_obj.is_masked());
    }
}
