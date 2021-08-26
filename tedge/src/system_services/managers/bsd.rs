use crate::system_services::SystemService::*;
use crate::system_services::*;
use std::process::*;
use tedge_users::{UserManager, ROOT_USER};

/// Service manager that uses `service(8)` as found on FreeBSD to control system services.
///
#[derive(Debug)]
pub struct BsdServiceManager {
    user_manager: UserManager,
}

impl BsdServiceManager {
    pub fn new(user_manager: UserManager) -> Self {
        Self { user_manager }
    }
}

impl SystemServiceManager for BsdServiceManager {
    fn name(&self) -> &str {
        "service(8)"
    }

    fn check_operational(&self) -> Result<(), SystemServiceError> {
        let mut command = ServiceCommand::CheckManager.into_command();

        match command.status() {
            Ok(status) if status.success() => Ok(()),
            _ => Err(BsdServiceError::ServiceManagerNotAvailable.into()),
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

impl BsdServiceManager {
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
            Err(BsdServiceError::ServiceCommandFailed {
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

const SERVICE_BIN: &str = "/usr/sbin/service";

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
            Self::CheckManager => format!("{} -l", SERVICE_BIN),
            Self::Stop(service) => format!(
                "{} {} stop",
                SERVICE_BIN,
                SystemService::as_service_name(service)
            ),
            Self::Restart(service) => {
                format!(
                    "{} {} restart",
                    SERVICE_BIN,
                    SystemService::as_service_name(service)
                )
            }
            Self::Enable(service) => {
                format!(
                    "{} {} enable",
                    SERVICE_BIN,
                    SystemService::as_service_name(service)
                )
            }
            Self::Disable(service) => {
                format!(
                    "{} {} forcedisable",
                    SERVICE_BIN,
                    SystemService::as_service_name(service)
                )
            }
            Self::IsActive(service) => {
                format!(
                    "{} {} status",
                    SERVICE_BIN,
                    SystemService::as_service_name(service)
                )
            }
        }
    }

    fn into_command(self) -> std::process::Command {
        match self {
            Self::CheckManager => CommandBuilder::new(SERVICE_BIN).arg("-l").silent().build(),
            Self::Stop(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(SystemService::as_service_name(service))
                .arg("stop")
                .silent()
                .build(),
            Self::Restart(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(SystemService::as_service_name(service))
                .arg("restart")
                .silent()
                .build(),
            Self::Enable(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(SystemService::as_service_name(service))
                .arg("enable")
                .silent()
                .build(),

            Self::Disable(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(SystemService::as_service_name(service))
                //
                // Use "forcedisable" as otherwise it could fail if you have a commented out
                // `# mosquitto_enable="YES"` or
                // `# mosquitto_enable="NO"` in your `/etc/rc.conf` file.
                //
                .arg("forcedisable")
                .silent()
                .build(),
            Self::IsActive(service) => CommandBuilder::new(SERVICE_BIN)
                .arg(SystemService::as_service_name(service))
                .arg("status")
                .silent()
                .build(),
        }
    }
}
