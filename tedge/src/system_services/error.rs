#[derive(thiserror::Error, Debug)]
pub enum ServicesError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    SystemdError(#[from] SystemdError),

    #[error(transparent)]
    PathsError(#[from] crate::utils::paths::PathsError),

    #[error("Unexpected value for exit status.")]
    UnexpectedExitStatus,
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
