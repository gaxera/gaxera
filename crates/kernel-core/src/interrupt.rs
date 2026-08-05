use crate::object::ObjectId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InterruptError {
    InvalidState,
    NotBound,
    AlreadyBound,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptTrigger {
    Level,
    Edge,
}

/// First-Class Interrupt Capability (`ObjectType::InterruptObject = 7`).
///
/// Represents exclusive capability authority over a specific hardware IRQ line.
pub struct InterruptObject {
    id: ObjectId,
    vector: u8,
    irq: u8,
    generation: u32,
    trigger: InterruptTrigger,
    owner_process: Option<ObjectId>,
    bound_notification: Option<ObjectId>,
    capability_refs: u32,
    masked: bool,
    in_flight: bool,
    closed: bool,
}

impl InterruptObject {
    pub fn new(id: ObjectId, vector: u8, irq: u8) -> Self {
        Self {
            id,
            vector,
            irq,
            generation: 1,
            trigger: InterruptTrigger::Level,
            owner_process: None,
            bound_notification: None,
            capability_refs: 1,
            masked: true,
            in_flight: false,
            closed: false,
        }
    }

    pub fn with_metadata(
        id: ObjectId,
        vector: u8,
        irq: u8,
        generation: u32,
        trigger: InterruptTrigger,
        owner_process: Option<ObjectId>,
    ) -> Self {
        let mut object = Self::new(id, vector, irq);
        object.generation = generation;
        object.trigger = trigger;
        object.owner_process = owner_process;
        object
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

    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn trigger(&self) -> InterruptTrigger {
        self.trigger
    }

    pub fn owner_process(&self) -> Option<ObjectId> {
        self.owner_process
    }

    pub fn is_masked(&self) -> bool {
        self.masked
    }

    pub fn bound_notification(&self) -> Option<ObjectId> {
        self.bound_notification
    }

    pub fn inc_capability_ref(&mut self) -> Result<(), InterruptError> {
        self.capability_refs = self
            .capability_refs
            .checked_add(1)
            .ok_or(InterruptError::Closed)?;
        Ok(())
    }

    pub fn dec_capability_ref(&mut self) -> Result<bool, InterruptError> {
        self.capability_refs = self
            .capability_refs
            .checked_sub(1)
            .ok_or(InterruptError::Closed)?;
        Ok(self.capability_refs == 0)
    }

    pub fn in_flight(&self) -> bool {
        self.in_flight
    }

    pub fn bind_notification(&mut self, notification: ObjectId) -> Result<(), InterruptError> {
        if self.closed {
            return Err(InterruptError::Closed);
        }
        if self.bound_notification.is_some() {
            return Err(InterruptError::AlreadyBound);
        }
        self.bound_notification = Some(notification);
        Ok(())
    }

    pub fn unbind_notification(&mut self) -> Result<(), InterruptError> {
        if self.closed {
            return Err(InterruptError::Closed);
        }
        if self.bound_notification.take().is_none() {
            return Err(InterruptError::NotBound);
        }
        self.in_flight = false;
        self.masked = true;
        Ok(())
    }

    pub fn mask(&mut self) {
        self.masked = true;
    }

    pub fn unmask(&mut self) {
        if !self.closed && self.bound_notification.is_some() && !self.in_flight {
            self.masked = false;
        }
    }

    pub fn begin_delivery(&mut self) -> Result<Option<ObjectId>, InterruptError> {
        if self.closed {
            return Err(InterruptError::Closed);
        }
        let notification = self.bound_notification.ok_or(InterruptError::NotBound)?;
        if self.masked || self.in_flight {
            return Ok(None);
        }
        self.masked = true;
        self.in_flight = true;
        Ok(Some(notification))
    }

    pub fn acknowledge(&mut self) -> Result<(), InterruptError> {
        if self.closed {
            return Err(InterruptError::Closed);
        }
        if !self.in_flight {
            return Err(InterruptError::InvalidState);
        }
        self.in_flight = false;
        Ok(())
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.masked = true;
        self.in_flight = false;
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
