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

    #[error("Service Manager: '{0}' is not available on the system or elevated permissions have not been granted.")]
    ServiceManagerUnavailable(String),
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

    #[error("Returned exit code: '{code:?}' for: systemd' is unhandled.")]
    UnhandledReturnCode { code: i32 },
}

/// The error type used by the `OpenRcServiceManager`
#[derive(thiserror::Error, Debug)]
pub enum OpenRcServiceError {
    #[error("Service command <{service_command:?}> failed with code: {code:?}.")]
    ServiceCommandFailed {
        service_command: String,
        code: Option<i32>,
    },
}

/// The error type used by the `BsdServiceManager`
#[derive(thiserror::Error, Debug)]
pub enum BsdServiceError {
    #[error("Service command <{service_command:?}> failed with code: {code:?}.")]
    ServiceCommandFailed {
        service_command: String,
        code: Option<i32>,
    },
}
