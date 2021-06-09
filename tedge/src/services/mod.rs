use crate::services::SystemdError::{
    ServiceNotFound, ServiceNotLoaded, SystemdNotAvailable, UnhandledReturnCode, UnspecificError,
};
use crate::system_commands::*;
use crate::utils::paths;

pub mod mosquitto;
pub mod tedge_mapper_az;
pub mod tedge_mapper_c8y;

type ExitCode = i32;

const SYSTEMCTL_OK: ExitCode = 0;
const SYSTEMCTL_ERROR_GENERIC: ExitCode = 1;
const SYSTEMCTL_ERROR_UNIT_IS_NOT_ACTIVE: ExitCode = 3;
const SYSTEMCTL_ERROR_SERVICE_NOT_FOUND: ExitCode = 5;
const SYSTEMCTL_ERROR_SERVICE_NOT_LOADED: ExitCode = 5;

pub trait SystemdService {
    const SERVICE_NAME: &'static str;

    fn stop(
        &self,
        system_command_runner: &dyn AbstractSystemCommandRunner,
    ) -> Result<(), ServicesError> {
        let command = SystemdStopService {
            service_name: Self::SERVICE_NAME.into(),
        };
        let code = system_command_runner
            .run(command)?
            .code()
            .ok_or(ServicesError::UnexpectedExitStatus)?;

        match code {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(UnspecificError {
                service: Self::SERVICE_NAME,
                cmd: "stop",
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
    fn restart(
        &self,
        system_command_runner: &dyn AbstractSystemCommandRunner,
    ) -> Result<(), ServicesError> {
        let command = SystemdRestartService {
            service_name: Self::SERVICE_NAME.into(),
        };
        let code = system_command_runner
            .run(command)?
            .code()
            .ok_or(ServicesError::UnexpectedExitStatus)?;

        match code {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(UnspecificError {
                service: Self::SERVICE_NAME,
                cmd: "restart",
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

    fn enable(
        &self,
        system_command_runner: &dyn AbstractSystemCommandRunner,
    ) -> Result<(), ServicesError> {
        let command = SystemdEnableService {
            service_name: Self::SERVICE_NAME.into(),
        };
        let code = system_command_runner
            .run(command)?
            .code()
            .ok_or(ServicesError::UnexpectedExitStatus)?;

        match code {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(UnspecificError {
                service: Self::SERVICE_NAME,
                cmd: "enable",
                hint: "Lacking permissions.",
            })),
            code => Err(ServicesError::SystemdError(UnhandledReturnCode { code })),
        }
    }

    fn disable(
        &self,
        system_command_runner: &dyn AbstractSystemCommandRunner,
    ) -> Result<(), ServicesError> {
        let command = SystemdDisableService {
            service_name: Self::SERVICE_NAME.into(),
        };
        let code = system_command_runner
            .run(command)?
            .code()
            .ok_or(ServicesError::UnexpectedExitStatus)?;

        match code {
            SYSTEMCTL_OK => Ok(()),
            SYSTEMCTL_ERROR_GENERIC => Err(ServicesError::SystemdError(UnspecificError {
                service: Self::SERVICE_NAME,
                cmd: "disable",
                hint: "Lacking permissions.",
            })),
            code => Err(ServicesError::SystemdError(UnhandledReturnCode { code })),
        }
    }

    fn is_active(
        &self,
        system_command_runner: &dyn AbstractSystemCommandRunner,
    ) -> Result<bool, ServicesError> {
        let command = SystemdIsServiceActive {
            service_name: Self::SERVICE_NAME.into(),
        };
        let code = system_command_runner
            .run(command)?
            .code()
            .ok_or(ServicesError::UnexpectedExitStatus)?;

        match code {
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
    SystemCommandError(#[from] SystemCommandError),

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

    #[error("Unexpected value for exit status.")]
    UnexpectedExitStatus,
}

pub(crate) fn systemd_available(
    system_command_runner: &dyn AbstractSystemCommandRunner,
) -> Result<(), ServicesError> {
    match system_command_runner.run(SystemdVersion) {
        Ok(status) if status.success() => Ok(()),
        _ => Err(ServicesError::SystemdError(SystemdNotAvailable)),
    }
}
