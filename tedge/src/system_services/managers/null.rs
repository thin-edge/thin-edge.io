use crate::system_services::{SystemService, SystemServiceError, SystemServiceManager};

/// A system service manager that always fails.
#[derive(Debug)]
pub struct NullSystemServiceManager;

impl SystemServiceManager for NullSystemServiceManager {
    fn name(&self) -> &str {
        "null (no service manager)"
    }

    fn check_operational(&self) -> Result<(), SystemServiceError> {
        Err(SystemServiceError::UnsupportedOperation)
    }

    fn stop_service(&self, _service: SystemService) -> Result<(), SystemServiceError> {
        Err(SystemServiceError::UnsupportedOperation)
    }

    fn restart_service(&self, _service: SystemService) -> Result<(), SystemServiceError> {
        Err(SystemServiceError::UnsupportedOperation)
    }

    fn enable_service(&self, _service: SystemService) -> Result<(), SystemServiceError> {
        Err(SystemServiceError::UnsupportedOperation)
    }

    fn disable_service(&self, _service: SystemService) -> Result<(), SystemServiceError> {
        Err(SystemServiceError::UnsupportedOperation)
    }

    fn is_service_running(&self, _service: SystemService) -> Result<bool, SystemServiceError> {
        Err(SystemServiceError::UnsupportedOperation)
    }
}
