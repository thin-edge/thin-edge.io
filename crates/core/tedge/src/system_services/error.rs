#[derive(thiserror::Error, Debug)]
pub enum SystemServiceError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Service command <{service_command:?}> failed with code: {code:?}.")]
    ServiceCommandFailed {
        service_command: String,
        code: Option<i32>,
    },

    #[error("Service Manager: '{0}' is not available on the system or elevated permissions have not been granted.")]
    ServiceManagerUnavailable(String),

    #[error(transparent)]
    FromSystemConfig(#[from] SystemConfigError),
}

#[derive(thiserror::Error, Debug)]
pub enum SystemConfigError {
    #[error("System config file not found: {0}")]
    ConfigFileNotFound(std::path::PathBuf),

    #[error("Invalid syntax in the system config file: {reason}")]
    InvalidSyntax { reason: String },
}
