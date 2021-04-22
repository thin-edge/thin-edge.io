use crate::system_services::*;
use crate::utils::users::{UserManager, ROOT_USER};
use service_command::*;
use std::process::*;

mod service_command;

/// Service manager that uses [OpenRc][1] to control system services.
///
/// [1]: https://github.com/OpenRc/openrc
///
pub struct OpenRcServiceManager {
    user_manager: UserManager,
}

impl OpenRcServiceManager {
    pub fn new(user_manager: UserManager) -> Self {
        Self { user_manager }
    }
}

impl SystemServiceManager for OpenRcServiceManager {
    fn manager_name(&self) -> &str {
        "OpenRC"
    }

    fn check_manager_available(&mut self) -> Result<(), ServicesError> {
        let mut command = ServiceCommand::CheckManager.into_command();

        match command.status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(OpenRcServiceError::ServiceManagerNotAvailable.into()),
        }
    }

    fn stop_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_command = ServiceCommand::Stop(service);

        self.run_service_command_as_root(service_command)?
            .must_succeed()
    }

    fn restart_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_command = ServiceCommand::Restart(service);

        self.run_service_command_as_root(service_command)?
            .must_succeed()
    }

    fn enable_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_command = ServiceCommand::Enable(service);

        self.run_service_command_as_root(service_command)?
            .must_succeed()
    }

    fn disable_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_command = ServiceCommand::Disable(service);

        self.run_service_command_as_root(service_command)?
            .must_succeed()
    }

    fn is_service_active(&mut self, service: SystemService) -> Result<bool, ServicesError> {
        let service_command = ServiceCommand::IsActive(service);

        self.run_service_command_as_root(service_command)
            .map(|status| status.success())
    }
}

impl OpenRcServiceManager {
    fn run_service_command_as_root(
        &self,
        service_command: ServiceCommand,
    ) -> Result<ServiceCommandExitStatus, ServicesError> {
        let _root_guard = self.user_manager.become_user(ROOT_USER);

        service_command
            .into_command()
            .status()
            .map_err(Into::into)
            .map(|status| ServiceCommandExitStatus {
                status,
                service_command,
            })
    }
}

struct ServiceCommandExitStatus {
    status: ExitStatus,
    service_command: ServiceCommand,
}

impl ServiceCommandExitStatus {
    fn must_succeed(self) -> Result<(), ServicesError> {
        if self.status.success() {
            Ok(())
        } else {
            Err(OpenRcServiceError::ServiceCommandFailed {
                service_command: self.service_command.to_string(),
                code: self.status.code(),
            }
            .into())
        }
    }

    fn success(self) -> bool {
        self.status.success()
    }
}
