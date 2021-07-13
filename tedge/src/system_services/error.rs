#[derive(thiserror::Error, Debug)]
pub enum SystemServiceError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    SystemdError(#[from] SystemdError),

    #[error(transparent)]
    OpenRcServiceError(#[from] OpenRcServiceError),

    #[error(transparent)]
    BsdServiceError(#[from] BsdServiceError),

    #[error("Unexpected value for exit status.")]
    UnexpectedExitStatus,

    #[error("Unsupported operation.")]
    UnsupportedOperation,
}

/// The error type used by the `SystemdServiceManager`
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

/// The error type used by the `OpenRcServiceManager`
#[derive(thiserror::Error, Debug)]
pub enum OpenRcServiceError {
    #[error("OpenRC is not available on the system.")]
    ServiceManagerNotAvailable,

    #[error("Service command <{service_command:?}> failed with code: {code:?}.")]
    ServiceCommandFailed {
        service_command: String,
        code: Option<i32>,
    },
}

/// The error type used by the `BsdServiceManager`
#[derive(thiserror::Error, Debug)]
pub enum BsdServiceError {
    #[error("service(8) is not available on the system.")]
    ServiceManagerNotAvailable,

    #[error("Service command <{service_command:?}> failed with code: {code:?}.")]
    ServiceCommandFailed {
        service_command: String,
        code: Option<i32>,
    },
}
