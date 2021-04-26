use crate::system_command::*;
use crate::system_services::*;
use std::sync::Arc;

type ExitCode = i32;

const SYSTEMCTL_OK: ExitCode = 0;
const SYSTEMCTL_ERROR_GENERIC: ExitCode = 1;
const SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE: ExitCode = 3;
const SYSTEMCTL_ERROR_SERVICE_NOT_FOUND: ExitCode = 5;
const SYSTEMCTL_ERROR_SERVICE_NOT_LOADED: ExitCode = 5;

const SYSTEMCTL_BIN: &str = "systemctl";

pub struct SystemdManager {
    systemctl_bin: String,
    system_command_runner: Arc<dyn SystemCommandRunner>,
}

impl SystemdManager {
    pub fn new(system_command_runner: Arc<dyn SystemCommandRunner>) -> Self {
        Self {
            systemctl_bin: SYSTEMCTL_BIN.into(),
            system_command_runner,
        }
    }
}

impl SystemServiceManager for SystemdManager {
    fn manager_name(&self) -> &str {
        "systemd"
    }

    fn check_manager_available(&mut self) -> Result<(), ServicesError> {
        let system_command =
            SystemCommand::new(&self.systemctl_bin).arg(SystemCtlParam::Version.as_str());
        match self.system_command_runner.run(system_command) {
            Ok(status) if status.success() => Ok(()),
            _ => Err(SystemdError::SystemdNotAvailable.into()),
        }
    }

    fn stop_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_name = service.as_service_name();
        let system_command = SystemCommand::new(&self.systemctl_bin)
            .arg(SystemCtlCmd::Stop)
            .arg(service_name)
            .role(Role::Root);
        match self.run_system_command(system_command)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(SystemdError::UnspecificError {
                service: service_name,
                cmd: SystemCtlCmd::Stop.as_str(),
                hint: "Lacking permissions.",
            }
            .into()),
            SYSTEMCTL_ERROR_SERVICE_NOT_LOADED => Err(SystemdError::ServiceNotLoaded {
                service: service_name,
            }
            .into()),
            code => Err(SystemdError::UnhandledReturnCode { code }.into()),
        }
    }

    // Note that restarting a unit with this command does not necessarily flush out all of the unit's resources before it is started again.
    // For example, the per-service file descriptor storage facility (see FileDescriptorStoreMax= in systemd.service(5)) will remain intact
    // as long as the unit has a job pending, and is only cleared when the unit is fully stopped and no jobs are pending anymore.
    // If it is intended that the file descriptor store is flushed out, too, during a restart operation an explicit
    // systemctl stop command followed by systemctl start should be issued.
    fn restart_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_name = service.as_service_name();
        let system_command = SystemCommand::new(&self.systemctl_bin)
            .arg(SystemCtlCmd::Restart)
            .arg(service_name)
            .role(Role::Root);

        match self.run_system_command(system_command)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(SystemdError::UnspecificError {
                service: service_name,
                cmd: SystemCtlCmd::Restart.as_str(),
                hint: "Lacking permissions or service's process exited with error code.",
            }
            .into()),
            SYSTEMCTL_ERROR_SERVICE_NOT_FOUND => Err(SystemdError::ServiceNotFound {
                service: service_name,
            }
            .into()),
            code => Err(SystemdError::UnhandledReturnCode { code }.into()),
        }
    }

    fn enable_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_name = service.as_service_name();
        let system_command = SystemCommand::new(&self.systemctl_bin)
            .arg(SystemCtlCmd::Enable)
            .arg(service_name)
            .role(Role::Root);

        match self.run_system_command(system_command)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(SystemdError::UnspecificError {
                service: service_name,
                cmd: SystemCtlCmd::Enable.as_str(),
                hint: "Lacking permissions.",
            }
            .into()),
            code => Err(SystemdError::UnhandledReturnCode { code }.into()),
        }
    }

    fn disable_service(&mut self, service: SystemService) -> Result<(), ServicesError> {
        let service_name = service.as_service_name();
        let system_command = SystemCommand::new(&self.systemctl_bin)
            .arg(SystemCtlCmd::Disable)
            .arg(service_name)
            .role(Role::Root);

        match self.run_system_command(system_command)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(SystemdError::UnspecificError {
                service: service_name,
                cmd: SystemCtlCmd::Disable.as_str(),
                hint: "Lacking permissions.",
            }
            .into()),
            code => Err(SystemdError::UnhandledReturnCode { code }.into()),
        }
    }

    fn is_service_active(&mut self, service: SystemService) -> Result<bool, ServicesError> {
        let service_name = service.as_service_name();

        let system_command = SystemCommand::new(&self.systemctl_bin)
            .arg(SystemCtlCmd::IsActive)
            .arg(service_name);

        match self.run_system_command(system_command)? {
            SYSTEMCTL_OK => Ok(true),
            SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE => Ok(false),
            code => Err(SystemdError::UnhandledReturnCode { code }.into()),
        }
    }
}

impl SystemdManager {
    fn run_system_command(&self, system_command: SystemCommand) -> Result<i32, ServicesError> {
        self.system_command_runner
            .run(system_command)?
            .code()
            .ok_or(ServicesError::UnexpectedExitStatus)
    }
}

#[derive(Debug, Copy, Clone)]
enum SystemCtlCmd {
    Enable,
    Disable,
    IsActive,
    Stop,
    Restart,
}

impl SystemCtlCmd {
    fn as_str(&self) -> &'static str {
        match self {
            SystemCtlCmd::Enable => "enable",
            SystemCtlCmd::Disable => "disable",
            SystemCtlCmd::IsActive => "is-active",
            SystemCtlCmd::Stop => "stop",
            SystemCtlCmd::Restart => "restart",
        }
    }
}

impl Into<String> for SystemCtlCmd {
    fn into(self) -> String {
        self.as_str().into()
    }
}

#[derive(Debug, Copy, Clone)]
enum SystemCtlParam {
    Version,
}

impl SystemCtlParam {
    fn as_str(&self) -> &'static str {
        match self {
            SystemCtlParam::Version => "--version",
        }
    }
}

impl Into<String> for SystemCtlParam {
    fn into(self) -> String {
        self.as_str().into()
    }
}
