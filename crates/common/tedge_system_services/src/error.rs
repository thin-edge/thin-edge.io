use camino::Utf8PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum SystemServiceError {
    #[error("Service command <{service_command:?}> failed with code {code}: {reason}")]
    ServiceCommandFailedWithCode {
        service_command: String,
        code: i32,
        reason: String,
    },

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
}

/// The last line written to stderr, used as the reason of a failure
pub fn failure_reason(stderr: &str) -> String {
    match stderr.lines().rfind(|line| !line.trim().is_empty()) {
        Some(reason) => reason.trim().to_string(),
        None => "no reason given".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_line_of_stderr_is_the_reason() {
        assert_eq!(
            failure_reason("Starting nginx\nUnit nginx.service not found.\n\n"),
            "Unit nginx.service not found."
        );
    }

    #[test]
    fn a_silent_command_gives_no_reason() {
        assert_eq!(failure_reason("  \n\n"), "no reason given");
    }
}
