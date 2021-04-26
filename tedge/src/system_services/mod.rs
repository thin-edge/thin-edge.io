pub mod bsd_service;
pub mod error;
pub mod factory;
pub mod openrc;
pub mod systemd;

pub use self::{error::*, factory::*};

#[derive(Debug, Copy, Clone)]
pub enum SystemService {
    Mosquitto,
    TEdgeMapper,
}

impl SystemService {
    pub fn as_service_name(&self) -> &'static str {
        match self {
            SystemService::Mosquitto => "mosquitto",
            SystemService::TEdgeMapper => "tedge-mapper",
        }
    }
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
