use camino::Utf8Path;

use crate::system_services::*;
use std::fmt::Debug;
use std::sync::Arc;

/// Abstraction over the system-provided facility that manages starting, stopping as well as other
/// service-related management functions of system services.
pub trait SystemServiceManager: Debug {
    /// Returns the name of the system service manager facility (e.g. "systemd" or "openrc").
    fn name(&self) -> &str;

    /// Checks whether the system service manager facility is available and operational.
    fn check_operational(&self) -> Result<(), SystemServiceError>;

    /// Stops the specified system service.
    fn stop_service(&self, service: SystemService) -> Result<(), SystemServiceError>;

    /// Restarts the specified system service.
    fn restart_service(&self, service: SystemService) -> Result<(), SystemServiceError>;

    /// Enables the specified system service. This does not start the service, unless you reboot.
    fn enable_service(&self, service: SystemService) -> Result<(), SystemServiceError>;

    /// Disables the specified system service. This does not stop the service.
    fn disable_service(&self, service: SystemService) -> Result<(), SystemServiceError>;

    /// Queries status of the specified system service. "Running" here means the same as "active".
    fn is_service_running(&self, service: SystemService) -> Result<bool, SystemServiceError>;

    /// Utility method that only restarts the `service` if it is already running.
    fn restart_service_if_running(
        &self,
        service: SystemService,
    ) -> Result<bool, SystemServiceError> {
        if self.is_service_running(service)? {
            self.restart_service(service)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

pub fn service_manager(
    config_root: &Utf8Path,
) -> Result<Arc<dyn SystemServiceManager>, SystemServiceError> {
    Ok(Arc::new(GeneralServiceManager::try_new(config_root)?))
}
