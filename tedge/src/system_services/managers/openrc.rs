use crate::system_services::*;
use std::process::*;
use tedge_users::{UserManager, ROOT_USER};

/// Service manager that uses [OpenRC][1] to control system services.
///
/// [1]: https://github.com/OpenRc/openrc
///
#[derive(Debug)]
pub struct OpenRcServiceManager {
    user_manager: UserManager,
}

impl OpenRcServiceManager {
    pub fn new(user_manager: UserManager) -> Self {
        Self { user_manager }
    }
}

impl SystemServiceManager for OpenRcServiceManager {
    fn name(&self) -> &str {
        "OpenRC"
    }

    fn check_operational(&self) -> Result<(), SystemServiceError> {
        let mut command = ServiceCommand::CheckManager.into_command();

        match command.status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(OpenRcServiceError::ServiceManagerNotAvailable.into()),
        }
    }

    fn stop_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let service_command = ServiceCommand::Stop(service);

        self.run_service_command_as_root(service_command)?
            .must_succeed()
    }

    fn restart_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let service_command = ServiceCommand::Restart(service);

        self.run_service_command_as_root(service_command)?
            .must_succeed()
    }

    fn enable_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let service_command = ServiceCommand::Enable(service);

        self.run_service_command_as_root(service_command)?
            .must_succeed()
    }

    fn disable_service(&self, service: SystemService) -> Result<(), SystemServiceError> {
        let service_command = ServiceCommand::Disable(service);

        self.run_service_command_as_root(service_command)?
            .must_succeed()
    }

    fn is_service_running(&self, service: SystemService) -> Result<bool, SystemServiceError> {
        let service_command = ServiceCommand::IsActive(service);

        self.run_service_command_as_root(service_command)
            .map(|status| status.success())
    }
}

impl OpenRcServiceManager {
    fn run_service_command_as_root(
        &self,
        service_command: ServiceCommand,
    ) -> Result<ServiceCommandExitStatus, SystemServiceError> {
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
    fn must_succeed(self) -> Result<(), SystemServiceError> {
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

const RC_SERVICE_BIN: &str = "/sbin/rc-service";
const RC_UPDATE_BIN: &str = "/sbin/rc-update";

#[derive(Debug, Copy, Clone)]
enum ServiceCommand {
    CheckManager,
    Stop(SystemService),
    Restart(SystemService),
    Enable(SystemService),
    Disable(SystemService),
    IsActive(SystemService),
}

impl ServiceCommand {
    fn to_string(self) -> String {
        match self {
            Self::CheckManager => format!("{} -l", RC_SERVICE_BIN),
            Self::Stop(service) => format!("{} {} stop", RC_SERVICE_BIN, as_service_name(service)),
            Self::Restart(service) => {
                format!("{} {} restart", RC_SERVICE_BIN, as_service_name(service))
            }
            Self::Enable(service) => format!("{} add {}", RC_UPDATE_BIN, as_service_name(service)),
            Self::Disable(service) => {
                format!("{} delete {}", RC_UPDATE_BIN, as_service_name(service))
            }
            Self::IsActive(service) => {
                format!("{} {} status", RC_SERVICE_BIN, as_service_name(service))
            }
        }
    }

    fn into_command(self) -> std::process::Command {
        match self {
            Self::CheckManager => CommandBuilder::new(RC_SERVICE_BIN)
                .arg("-l")
                .silent()
                .build(),
            Self::Stop(service) => CommandBuilder::new(RC_SERVICE_BIN)
                .arg(as_service_name(service))
                .arg("stop")
                .silent()
                .build(),
            Self::Restart(service) => CommandBuilder::new(RC_SERVICE_BIN)
                .arg(as_service_name(service))
                .arg("restart")
                .silent()
                .build(),
            Self::Enable(service) => CommandBuilder::new(RC_UPDATE_BIN)
                .arg("add")
                .arg(as_service_name(service))
                .silent()
                .build(),
            Self::Disable(service) => CommandBuilder::new(RC_SERVICE_BIN)
                .arg("delete")
                .arg(as_service_name(service))
                .silent()
                .build(),
            Self::IsActive(service) => CommandBuilder::new(RC_SERVICE_BIN)
                .arg(as_service_name(service))
                .arg("status")
                .silent()
                .build(),
        }
    }
}

fn as_service_name(service: SystemService) -> &'static str {
    match service {
        SystemService::Mosquitto => "mosquitto",
        SystemService::TEdgeMapperAz => "tedge-mapper-az",
        SystemService::TEdgeMapperC8y => "tedge-mapper-c8y",
    }
}
