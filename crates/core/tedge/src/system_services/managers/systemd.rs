use crate::system_services::*;
use std::process::ExitStatus;
use tedge_users::*;

#[derive(Debug)]
pub struct SystemdServiceManager {
    systemctl_bin: String,
    user_manager: UserManager,
}

type ExitCode = i32;
type Error = SystemServiceError;

const SYSTEMCTL_OK: ExitCode = 0;
const SYSTEMCTL_ERROR_GENERIC: ExitCode = 1;
const SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE: ExitCode = 3;
const SYSTEMCTL_ERROR_SERVICE_NOT_FOUND: ExitCode = 5;
const SYSTEMCTL_ERROR_SERVICE_NOT_LOADED: ExitCode = 5;

const SYSTEMCTL_BIN: &str = "systemctl";

impl SystemdServiceManager {
    pub fn new(user_manager: UserManager) -> Self {
        Self {
            systemctl_bin: SYSTEMCTL_BIN.into(),
            user_manager,
        }
    }
}

impl SystemServiceManager for SystemdServiceManager {
    fn name(&self) -> &str {
        "systemd"
    }

    fn check_operational(&self) -> Result<(), SystemServiceError> {
        match std::process::Command::new(&self.systemctl_bin)
            .arg(SystemCtlParam::Version.as_str())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        {
            Ok(status) if status.success() => Ok(()),
            _ => Err(SystemServiceError::ServiceManagerUnavailable(
                self.name().to_string(),
            )),
        }
    }

    fn stop_service(&self, service: SystemService) -> Result<(), Error> {
        let service_name = SystemService::as_service_name(service);
        match self.call_systemd_subcmd_sudo(SystemCtlCmd::Stop, service_name)? {
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
    fn restart_service(&self, service: SystemService) -> Result<(), Error> {
        let service_name = SystemService::as_service_name(service);
        match self.call_systemd_subcmd_sudo(SystemCtlCmd::Restart, service_name)? {
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

    fn enable_service(&self, service: SystemService) -> Result<(), Error> {
        let service_name = SystemService::as_service_name(service);
        match self.call_systemd_subcmd_sudo(SystemCtlCmd::Enable, service_name)? {
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

    fn disable_service(&self, service: SystemService) -> Result<(), Error> {
        let service_name = SystemService::as_service_name(service);
        match self.call_systemd_subcmd_sudo(SystemCtlCmd::Disable, service_name)? {
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

    fn is_service_running(&self, service: SystemService) -> Result<bool, Error> {
        let service_name = SystemService::as_service_name(service);
        match self.call_systemd_subcmd(SystemCtlCmd::IsActive, service_name)? {
            SYSTEMCTL_OK => Ok(true),
            SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE => Ok(false),
            code => Err(SystemdError::UnhandledReturnCode { code }.into()),
        }
    }
}

impl SystemdServiceManager {
    fn call_systemd_subcmd_sudo(
        &self,
        systemctl_subcmd: SystemCtlCmd,
        arg: &str,
    ) -> Result<i32, Error> {
        let _root_guard = self.user_manager.become_user(ROOT_USER);
        self.call_systemd_subcmd(systemctl_subcmd, arg)
    }

    fn call_systemd_subcmd(&self, systemctl_subcmd: SystemCtlCmd, arg: &str) -> Result<i32, Error> {
        cmd_nullstdio_args_with_code(&self.systemctl_bin, &[systemctl_subcmd.as_str(), arg])?
            .code()
            .ok_or(Error::UnexpectedExitStatus)
    }
}

fn cmd_nullstdio_args_with_code(command: &str, args: &[&str]) -> Result<ExitStatus, Error> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?)
}

#[derive(Debug)]
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

impl From<SystemCtlCmd> for String {
    fn from(val: SystemCtlCmd) -> Self {
        val.as_str().into()
    }
}

#[derive(Debug)]
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

impl From<SystemCtlParam> for String {
    fn from(val: SystemCtlParam) -> Self {
        val.as_str().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_nullstdio_args_expected_exit_code_zero() {
        // There is a chance that this may fail on very embedded system which will not have 'ls' command on busybox.
        assert_eq!(
            cmd_nullstdio_args_with_code("ls", &[]).unwrap().code(),
            Some(0)
        );
    }

    #[test]
    fn cmd_nullstdio_args_command_not_exists() {
        assert!(cmd_nullstdio_args_with_code("test-command", &[]).is_err())
    }
}
