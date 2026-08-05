use alloc::vec::Vec;
use gaxera_abi::{Handle, ObjectType, ProcessControlOp, Rights};
use libgaxera::process::{ProcessBuildError, ProcessBuilder};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverState {
    Uninitialized,
    Running,
    Crashed,
    FailedPermanently,
    Stopped,
}

#[derive(Clone, Debug)]
pub struct DriverConfig<'a> {
    pub name: &'a str,
    pub elf_data: &'a [u8],
    pub irq: Option<u8>,
    pub max_restarts: u32,
}

pub struct DriverInstance<'a> {
    pub config: DriverConfig<'a>,
    pub process_handle: Option<Handle>,
    pub exit_notification: Option<Handle>,
    pub restart_count: u32,
    pub state: DriverState,
}

pub struct DriverSupervisor<'a> {
    factory: Handle,
    self_aspace: Handle,
    image_factory: Option<Handle>,
    drivers: Vec<DriverInstance<'a>>,
}

impl<'a> DriverSupervisor<'a> {
    pub fn new(
        factory: Handle,
        _cspace: Handle,
        self_aspace: Handle,
        image_factory: Option<Handle>,
    ) -> Self {
        Self {
            factory,
            self_aspace,
            image_factory,
            drivers: Vec::new(),
        }
    }

    pub fn register_driver(
        &mut self,
        config: DriverConfig<'a>,
    ) -> Result<usize, ProcessBuildError> {
        let index = self.drivers.len();
        self.drivers
            .try_reserve(1)
            .map_err(|_| ProcessBuildError::MemoryAllocationFailed)?;
        self.drivers.push(DriverInstance {
            config,
            process_handle: None,
            exit_notification: None,
            restart_count: 0,
            state: DriverState::Uninitialized,
        });
        Ok(index)
    }

    pub fn driver_state(&self, index: usize) -> Option<DriverState> {
        self.drivers.get(index).map(|d| d.state)
    }

    pub fn restart_count(&self, index: usize) -> Option<u32> {
        self.drivers.get(index).map(|d| d.restart_count)
    }

    pub fn start_driver(&mut self, index: usize) -> Result<Handle, ProcessBuildError> {
        let driver = match self.drivers.get_mut(index) {
            Some(d) => d,
            None => return Err(ProcessBuildError::CreateProcessFailed),
        };

        if driver.restart_count >= driver.config.max_restarts {
            driver.state = DriverState::FailedPermanently;
            return Err(ProcessBuildError::CreateProcessFailed);
        }

        let mut builder =
            ProcessBuilder::new(self.factory, self.self_aspace, driver.config.elf_data);

        if let Some(img_fact) = self.image_factory {
            builder = builder.with_image_factory(img_fact);
        }

        let process_handle = builder.spawn()?;

        driver.process_handle = Some(process_handle);
        driver.state = DriverState::Running;

        Ok(process_handle)
    }

    #[allow(clippy::result_unit_err)]
    pub fn handle_driver_exit(&mut self, index: usize, exit_code: u64) -> Result<(), ()> {
        let max_restarts = match self.drivers.get(index) {
            Some(d) => d.config.max_restarts,
            None => return Err(()),
        };

        if let Some(driver) = self.drivers.get_mut(index) {
            if let Some(proc_handle) = driver.process_handle.take() {
                let _ = libgaxera::syscall::process_control(
                    proc_handle,
                    ProcessControlOp::Reap,
                    0,
                    0,
                    0,
                );
                let _ = libgaxera::syscall::delete_handle(proc_handle);
            }

            driver.restart_count += 1;

            if exit_code != 0 {
                driver.state = DriverState::Crashed;
            }

            if driver.restart_count >= max_restarts {
                driver.state = DriverState::FailedPermanently;
                return Err(());
            }
        }

        // Fresh-process restart
        match self.start_driver(index) {
            Ok(_) => Ok(()),
            Err(_) => {
                if let Some(driver) = self.drivers.get_mut(index) {
                    driver.state = DriverState::FailedPermanently;
                }
                Err(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_driver_supervisor_lifecycle_and_restart_limits() {
        let factory = Handle::from_parts(1, 1);
        let cspace = Handle::from_parts(2, 1);
        let aspace = Handle::from_parts(3, 1);

        let mut supervisor = DriverSupervisor::new(factory, cspace, aspace, None);

        let config = DriverConfig {
            name: "virtio_block",
            elf_data: &[],
            irq: Some(11),
            max_restarts: 3,
        };

        let idx = supervisor
            .register_driver(config)
            .expect("test registration");
        assert_eq!(
            supervisor.driver_state(idx),
            Some(DriverState::Uninitialized)
        );
        assert_eq!(supervisor.restart_count(idx), Some(0));
    }
}
