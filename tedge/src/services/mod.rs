use crate::services::SystemdError::{
    SystemdNotAvailable, SystemdServiceNotFound, SystemdServiceNotLoaded,
    SystemdUnhandledReturnCode, SystemdUnspecificError,
};
use crate::utils::paths;
use std::process::ExitStatus;
use which::which;

pub mod mosquitto;
pub mod tedge_mapper;

type ExitCode = i32;

const SYSTEMCTL_OK: ExitCode = 0;
const SYSTEMCTL_ERROR_GENERIC: ExitCode = 1;
const SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE: ExitCode = 3;
const SYSTEMCTL_ERROR_SERVICE_NOT_FOUND: ExitCode = 5;
const SYSTEMCTL_ERROR_SERVICE_NOT_LOADED: ExitCode = 5;

pub trait Service {
    const SERVICE_NAME: &'static str;

    fn stop(&self) -> Result<(), ServicesError> {
        match call_systemd_subcmd(SystemCtlCmd::Stop, Self::SERVICE_NAME)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(SystemdUnspecificError {
                service: Self::SERVICE_NAME,
                cmd: "stop",
                hint: "Lacking permissions.",
            })),
            SYSTEMCTL_ERROR_SERVICE_NOT_LOADED => {
                Err(ServicesError::SystemdError(SystemdServiceNotLoaded {
                    service: Self::SERVICE_NAME,
                }))
            }
            code => Err(ServicesError::SystemdError(SystemdUnhandledReturnCode {
                code,
            })),
        }
    }

    // Note that restarting a unit with this command does not necessarily flush out all of the unit's resources before it is started again.
    // For example, the per-service file descriptor storage facility (see FileDescriptorStoreMax= in systemd.service(5)) will remain intact
    // as long as the unit has a job pending, and is only cleared when the unit is fully stopped and no jobs are pending anymore.
    // If it is intended that the file descriptor store is flushed out, too, during a restart operation an explicit
    // systemctl stop command followed by systemctl start should be issued.
    fn restart(&self) -> Result<(), ServicesError> {
        match call_systemd_subcmd(SystemCtlCmd::Restart, Self::SERVICE_NAME)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(SystemdUnspecificError {
                service: Self::SERVICE_NAME,
                cmd: "restart",
                hint: "Lacking permissions or service's process exited with error code.",
            })),
            SYSTEMCTL_ERROR_SERVICE_NOT_FOUND => {
                Err(ServicesError::SystemdError(SystemdServiceNotFound {
                    service: Self::SERVICE_NAME,
                }))
            }
            code => Err(ServicesError::SystemdError(SystemdUnhandledReturnCode {
                code,
            })),
        }
    }

    fn enable(&self) -> Result<(), ServicesError> {
        match call_systemd_subcmd(SystemCtlCmd::Enable, Self::SERVICE_NAME)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(SystemdUnspecificError {
                service: Self::SERVICE_NAME,
                cmd: "enable",
                hint: "Lacking permissions.",
            })),
            code => Err(ServicesError::SystemdError(SystemdUnhandledReturnCode {
                code,
            })),
        }
    }

    fn disable(&self) -> Result<(), ServicesError> {
        match call_systemd_subcmd(SystemCtlCmd::Disable, Self::SERVICE_NAME)? {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(SystemdUnspecificError {
                service: Self::SERVICE_NAME,
                cmd: "disable",
                hint: "Lacking permissions.",
            })),
            code => Err(ServicesError::SystemdError(SystemdUnhandledReturnCode {
                code,
            })),
        }
    }

    fn is_active(&self) -> Result<bool, ServicesError> {
        match call_systemd_subcmd(SystemCtlCmd::IsActive, Self::SERVICE_NAME)? {
            SYSTEMCTL_OK => Ok(true),
            SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE => Ok(false),
            code => Err(ServicesError::SystemdError(SystemdUnhandledReturnCode {
                code,
            })),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SystemdError {
    #[error("Systemd returned unspecific error for service {service} while performing to {cmd} it.\nHint: {hint}")]
    SystemdUnspecificError {
        service: &'static str,
        cmd: &'static str,
        hint: &'static str,
    },

    #[error("Service {service} not found. Install {service} to use this command.")]
    SystemdServiceNotFound { service: &'static str },

    #[error("Service {service} not loaded.")]
    SystemdServiceNotLoaded { service: &'static str },

    #[error(
        "Systemd is not available on the system or elevated permissions have not been granted."
    )]
    SystemdNotAvailable,

    #[error("Returned exit code: '{code:?}' for: systemd' is unhandled.")]
    SystemdUnhandledReturnCode { code: i32 },
}

#[derive(thiserror::Error, Debug)]
pub enum ServicesError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    SystemdError(#[from] SystemdError),

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

    #[error("Couldn't find path to 'sudo'. Update $PATH variable with 'sudo' path.\n{0}")]
    SudoNotFound(#[from] which::Error),

    #[error("Unexpected value for exit status.")]
    UnexpectedExitStatus,
}

// Generic util functions
pub fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}

// Commands util functions
fn cmd_nullstdio_args_with_code_with_sudo(
    command: &str,
    args: &[&str],
) -> Result<ExitStatus, ServicesError> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?)
}

fn call_systemd_subcmd(systemctl_subcmd: SystemCtlCmd, arg: &str) -> Result<i32, ServicesError> {
    let sudo = paths::pathbuf_to_string(which("sudo")?)?;
    cmd_nullstdio_args_with_code_with_sudo(
        sudo.as_str(),
        &[SystemCtlCmd::Cmd.as_str(), systemctl_subcmd.as_str(), arg],
    )?
    .code()
    .ok_or(ServicesError::UnexpectedExitStatus)
}

pub(crate) fn systemd_available() -> Result<(), ServicesError> {
    std::process::Command::new(SystemCtlCmd::Cmd.as_str())
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
    Cmd,
    Enable,
    Disable,
    IsActive,
    Stop,
    Restart,
}

impl SystemCtlCmd {
    fn as_str(&self) -> &'static str {
        match self {
            SystemCtlCmd::Cmd => "systemctl",
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
        match self {
            SystemCtlCmd::Cmd => "systemctl".to_owned(),
            SystemCtlCmd::Enable => "enable".to_owned(),
            SystemCtlCmd::Disable => "disable".to_owned(),
            SystemCtlCmd::IsActive => "is-active".to_owned(),
            SystemCtlCmd::Stop => "stop".to_owned(),
            SystemCtlCmd::Restart => "restart".to_owned(),
        }
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

impl Into<String> for SystemCtlParam {
    fn into(self) -> String {
        match self {
            SystemCtlParam::Version => "--version".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn cmd_nullstdio_args_expected() {
        // There is a chance that this may fail on very embedded system which will not have 'ls' command on busybox.
        assert_eq!(cmd_nullstdio_args_with_code_with_sudo("ls", &[]).unwrap().code(), Some(0));

        if let Err(_err) = cmd_nullstdio_args_with_code_with_sudo("test-command", &[]) {
            panic!()
        }
    }
}
