use camino::Utf8PathBuf;

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

    #[error(
        "Syntax error in the system config file for '{cmd}': {reason}\n\
    Check '{path}' file."
    )]
    SystemConfigInvalidSyntax {
        reason: String,
        cmd: String,
        path: Utf8PathBuf,
    },

    #[error(
        "Action '{action}' is not supported by the '{manager}' init system.\n\
    Defined actions: {defined}.\n\
    Add a template for '{action}' to the [init] table of '{path}' to support it."
    )]
    UnsupportedAction {
        action: String,
        manager: String,
        defined: String,
        path: Utf8PathBuf,
    },

    #[error(
        "'{action}' is not a service action: the [init] table uses that key to describe the init \
    system.\n\
    Defined actions: {defined}."
    )]
    NotAnAction { action: String, defined: String },
}
