use crate::system_services::*;

#[derive(Debug)]
pub struct DummySystemServiceManager;

impl SystemServiceManager for DummySystemServiceManager {
    fn name(&self) -> &str {
        "dummy"
    }

    fn check_operational(&self) -> Result<(), SystemServiceError> {
        Ok(())
    }

    fn stop_service(&self, _service: SystemService) -> Result<(), SystemServiceError> {
        Ok(())
    }

    fn restart_service(&self, _service: SystemService) -> Result<(), SystemServiceError> {
        Ok(())
    }

    fn enable_service(&self, _service: SystemService) -> Result<(), SystemServiceError> {
        Ok(())
    }

    fn disable_service(&self, _service: SystemService) -> Result<(), SystemServiceError> {
        Ok(())
    }

    fn is_service_running(&self, _service: SystemService) -> Result<bool, SystemServiceError> {
        Ok(false)
    }
}
