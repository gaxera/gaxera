#[cfg(test)]
mod tests {
    use crate::interrupt::InterruptObject;
    use crate::notification::Notification;
    use crate::object::ObjectId;
    use crate::process::{Process, ProcessState};
    use crate::resource::{ResourceDomain, ResourceDomainId, ResourceLimits};

    fn test_id(index: u32) -> ObjectId {
        ObjectId::new_for_test(index, 1)
    }

    #[test]
    fn test_driver_crash_and_teardown_reclaims_resources() {
        let domain_id = ResourceDomainId::new_for_test(1);
        let limits = ResourceLimits {
            memory_bytes: 1024 * 1024,
            objects: 100,
            capabilities: 100,
        };
        let mut domain = ResourceDomain::new_for_test(domain_id, limits);

        // Charge quota for driver process memory
        assert!(domain.charge_memory(64 * 1024).is_ok());

        let proc_id = test_id(10);
        let mut process = Process::new(proc_id, domain_id);

        let irq_id = test_id(20);
        let mut irq_obj = InterruptObject::new(irq_id, 33, 1);
        let notif_id = test_id(21);
        let mut notif = Notification::new(notif_id);

        // Driver binds interrupt to notification
        assert_eq!(irq_obj.bind_notification(notif_id), Ok(()));
        irq_obj.unmask();
        assert!(!irq_obj.is_masked());

        // Driver crashes / exits
        assert!(process.request_exit(0xDEAD).is_ok());
        assert!(process.mark_exiting().is_ok());

        // Teardown masks IRQ and closes objects
        irq_obj.close();
        notif.close();

        assert!(irq_obj.is_masked());
        assert_eq!(irq_obj.bound_notification(), None);

        // Reclaim memory quota
        assert!(domain.rollback_memory(64 * 1024).is_ok());

        // Restart receives fresh process state
        let fresh_proc_id = test_id(11);
        let fresh_process = Process::new(fresh_proc_id, domain_id);
        assert_eq!(fresh_process.state(), ProcessState::New);
    }
}
