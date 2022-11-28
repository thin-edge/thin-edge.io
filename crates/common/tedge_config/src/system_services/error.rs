#[derive(thiserror::Error, Debug)]
pub enum SystemServiceError {
    #[error("Service command <{service_command:?}> failed with code: {code:?}.")]
    ServiceCommandFailedWithCode { service_command: String, code: i32 },

    #[error("Service command <{service_command:?}> terminated by a signal.")]
    ServiceCommandFailedBySignal { service_command: String },

    #[error(
        "Service command <{service_command:?}> not found.\n\
    Check '{path}' file."
    )]
    ServiceCommandNotFound {
        service_command: String,
        path: String,
    },

    #[error("Failed to execute '{cmd}' to check the service manager availability.\n\
     Service manager '{name}' is not available on the system or elevated permissions have not been granted.")]
    ServiceManagerUnavailable { cmd: String, name: String },

    #[error("Toml syntax error in the system config file '{path}': {reason}")]
    SystemConfigInvalidToml { path: String, reason: String },

    #[error(
        "Syntax error in the system config file for '{cmd}': {reason}\n\
    Check '{path}' file."
    )]
    SystemConfigInvalidSyntax {
        reason: String,
        cmd: String,
        path: String,
    },

    #[error("Invalid log level: {name:?}, supported levels are info, warn, error and debug")]
    InvalidLogLevel { name: String },
}
