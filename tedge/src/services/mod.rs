use crate::services::SystemdError::{
    ServiceNotFound, ServiceNotLoaded, SystemdNotAvailable, UnhandledReturnCode, UnspecificError,
};
use crate::utils::paths;
use crate::utils::users::UserManager;
use crate::utils::users::ROOT_USER;
use std::process::ExitStatus;

pub mod mosquitto;
pub mod tedge_mapper_az;
pub mod tedge_mapper_c8y;

type ExitCode = i32;

const SYSTEMCTL_OK: ExitCode = 0;
const SYSTEMCTL_ERROR_GENERIC: ExitCode = 1;
const SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE: ExitCode = 3;
const SYSTEMCTL_ERROR_SERVICE_NOT_FOUND: ExitCode = 5;
const SYSTEMCTL_ERROR_SERVICE_NOT_LOADED: ExitCode = 5;

const SYSTEMCTL_BIN: &str = "systemctl";

pub trait SystemdService {
    const SERVICE_NAME: &'static str;

    fn stop(&self, user_manager: &UserManager) -> Result<(), ServicesError> {
        match call_systemd_subcmd_sudo(SystemCtlCmd::Stop, Self::SERVICE_NAME, user_manager)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(UnspecificError {
                service: Self::SERVICE_NAME,
                cmd: SystemCtlCmd::Stop.as_str(),
                hint: "Lacking permissions.",
            })),
            SYSTEMCTL_ERROR_SERVICE_NOT_LOADED => {
                Err(ServicesError::SystemdError(ServiceNotLoaded {
                    service: Self::SERVICE_NAME,
                }))
            }
            code => Err(ServicesError::SystemdError(UnhandledReturnCode { code })),
        }
    }

    // Note that restarting a unit with this command does not necessarily flush out all of the unit's resources before it is started again.
    // For example, the per-service file descriptor storage facility (see FileDescriptorStoreMax= in systemd.service(5)) will remain intact
    // as long as the unit has a job pending, and is only cleared when the unit is fully stopped and no jobs are pending anymore.
    // If it is intended that the file descriptor store is flushed out, too, during a restart operation an explicit
    // systemctl stop command followed by systemctl start should be issued.
    fn restart(&self, user_manager: &UserManager) -> Result<(), ServicesError> {
        match call_systemd_subcmd_sudo(SystemCtlCmd::Restart, Self::SERVICE_NAME, user_manager)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(UnspecificError {
                service: Self::SERVICE_NAME,
                cmd: SystemCtlCmd::Restart.as_str(),
                hint: "Lacking permissions or service's process exited with error code.",
            })),
            SYSTEMCTL_ERROR_SERVICE_NOT_FOUND => {
                Err(ServicesError::SystemdError(ServiceNotFound {
                    service: Self::SERVICE_NAME,
                }))
            }
            code => Err(ServicesError::SystemdError(UnhandledReturnCode { code })),
        }
    }

    fn enable(&self, user_manager: &UserManager) -> Result<(), ServicesError> {
        match call_systemd_subcmd_sudo(SystemCtlCmd::Enable, Self::SERVICE_NAME, user_manager)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(UnspecificError {
                service: Self::SERVICE_NAME,
                cmd: SystemCtlCmd::Enable.as_str(),
                hint: "Lacking permissions.",
            })),
            code => Err(ServicesError::SystemdError(UnhandledReturnCode { code })),
        }
    }

    fn disable(&self, user_manager: &UserManager) -> Result<(), ServicesError> {
        match call_systemd_subcmd_sudo(SystemCtlCmd::Disable, Self::SERVICE_NAME, user_manager)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(UnspecificError {
                service: Self::SERVICE_NAME,
                cmd: SystemCtlCmd::Disable.as_str(),
                hint: "Lacking permissions.",
            })),
            code => Err(ServicesError::SystemdError(UnhandledReturnCode { code })),
        }
    }

    fn is_active(&self) -> Result<bool, ServicesError> {
        match call_systemd_subcmd(SystemCtlCmd::IsActive, Self::SERVICE_NAME)? {
            SYSTEMCTL_OK => Ok(true),
            SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE => Ok(false),
            code => Err(ServicesError::SystemdError(UnhandledReturnCode { code })),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SystemdError {
    #[error("Systemd returned unspecific error for service {service} while performing {cmd} it.\nHint: {hint}")]
    UnspecificError {
        service: &'static str,
        cmd: &'static str,
        hint: &'static str,
    },

    #[error("Service {service} not found. Install {service} to use this command.")]
    ServiceNotFound { service: &'static str },

    #[error("Service {service} not loaded.")]
    ServiceNotLoaded { service: &'static str },

    #[error(
        "Systemd is not available on the system or elevated permissions have not been granted."
    )]
    SystemdNotAvailable,

    #[error("Returned exit code: '{code:?}' for: systemd' is unhandled.")]
    UnhandledReturnCode { code: i32 },
}

#[derive(thiserror::Error, Debug)]
pub enum ServicesError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    SystemdError(#[from] SystemdError),

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

    #[error("Unexpected value for exit status.")]
    UnexpectedExitStatus,
}

fn cmd_nullstdio_args_with_code(command: &str, args: &[&str]) -> Result<ExitStatus, ServicesError> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?)
}

fn call_systemd_subcmd_sudo(
    systemctl_subcmd: SystemCtlCmd,
    arg: &str,
    user_manager: &UserManager,
) -> Result<i32, ServicesError> {
    let _root_guard = user_manager.become_user(ROOT_USER);
    call_systemd_subcmd(systemctl_subcmd, arg)
}

fn call_systemd_subcmd(systemctl_subcmd: SystemCtlCmd, arg: &str) -> Result<i32, ServicesError> {
    cmd_nullstdio_args_with_code(SYSTEMCTL_BIN, &[systemctl_subcmd.as_str(), arg])?
        .code()
        .ok_or(ServicesError::UnexpectedExitStatus)
}

pub(crate) fn systemd_available() -> Result<(), ServicesError> {
    std::process::Command::new(SYSTEMCTL_BIN)
        .arg(SystemCtlParam::Version.as_str())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_or_else(
            |_error| Err(ServicesError::SystemdError(SystemdNotAvailable)),
            |status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(ServicesError::SystemdError(SystemdNotAvailable))
                }
            },
        )
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
