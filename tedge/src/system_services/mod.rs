pub mod error;
pub mod systemd;

pub use self::error::*;
pub use self::systemd::SystemdServiceManager;

#[derive(Copy, Clone)]
pub enum SystemService {
    Mosquitto,
    TEdgeMapper,
}

/// The system facility that manages starting, stopping as well as other service-related management
/// functions of system services.
pub trait SystemServiceManager {
    /// Returns the name of the system service manager facility (e.g. "systemd" or "openrc").
    fn manager_name(&self) -> &str;

    /// Checks whether the system service manager facility is available.
    fn check_manager_available(&mut self) -> Result<(), ServicesError>;

    fn stop_service(&mut self, service: SystemService) -> Result<(), ServicesError>;
    fn restart_service(&mut self, service: SystemService) -> Result<(), ServicesError>;
    fn enable_service(&mut self, service: SystemService) -> Result<(), ServicesError>;
    fn disable_service(&mut self, service: SystemService) -> Result<(), ServicesError>;
    fn is_service_active(&mut self, service: SystemService) -> Result<bool, ServicesError>;

    fn restart_service_if_active(&mut self, service: SystemService) -> Result<bool, ServicesError> {
        if self.is_service_active(service)? {
            let () = self.restart_service(service)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
