use camino::Utf8Path;

use super::*;
use std::fmt::Debug;
use std::process::ExitStatus;
use std::sync::Arc;
use tedge_config::SystemTomlError;

#[derive(Debug, Default, Eq, PartialEq)]
pub struct ServiceCommandOutput {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub struct ServiceCommandOutcome {
    pub service_command: String,
    pub output: ServiceCommandOutput,
    pub status: ExitStatus,
}

impl ServiceCommandOutcome {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn must_succeed(self) -> Result<(), SystemServiceError> {
        if self.status.success() {
            return Ok(());
        }

        match self.status.code() {
            Some(code) => Err(SystemServiceError::ServiceCommandFailedWithCode {
                service_command: self.service_command,
                code,
            }),
            None => Err(SystemServiceError::ServiceCommandFailedBySignal {
                service_command: self.service_command,
            }),
        }
    }
}

/// Abstraction over the system-provided facility that manages starting, stopping as well as other
/// service-related management functions of system services.
#[async_trait::async_trait]
pub trait SystemServiceManager: Debug + Send + Sync {
    /// Returns the name of the system service manager facility (e.g. "systemd" or "openrc").
    fn name(&self) -> &str;

    /// Checks whether the system service manager facility is available and operational.
    async fn check_operational(&self) -> Result<(), SystemServiceError>;

    /// Runs an action defined in `system.toml` on the given service.
    async fn run_action(
        &self,
        action: &str,
        service: SystemService<'_>,
    ) -> Result<ServiceCommandOutcome, SystemServiceError>;

    /// Stops the specified system service.
    async fn stop_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        self.run_action("stop", service).await?.must_succeed()
    }

    /// Starts the specified system service.
    async fn start_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        self.run_action("start", service).await?.must_succeed()
    }

    /// Restarts the specified system service.
    async fn restart_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        self.run_action("restart", service).await?.must_succeed()
    }

    /// Enables the specified system service. This does not start the service, unless you reboot.
    async fn enable_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        self.run_action("enable", service).await?.must_succeed()
    }

    /// Disables the specified system service. This does not stop the service.
    async fn disable_service(&self, service: SystemService<'_>) -> Result<(), SystemServiceError> {
        self.run_action("disable", service).await?.must_succeed()
    }

    /// Queries status of the specified system service. "Running" here means the same as "active".
    async fn is_service_running(
        &self,
        service: SystemService<'_>,
    ) -> Result<bool, SystemServiceError>;
}

pub fn service_manager(
    config_root: &Utf8Path,
) -> Result<Arc<dyn SystemServiceManager>, SystemTomlError> {
    Ok(Arc::new(GeneralServiceManager::try_new(config_root)?))
}
