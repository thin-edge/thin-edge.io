/// Error raised related to execution of a `SystemCommand`.
#[derive(thiserror::Error, Debug)]
pub enum SystemCommandError {
    #[error("Insufficient permissions for running command.")]
    InsufficientPermissions,

    #[error("Failed to execute command")]
    CommandExecutionFailed(std::io::Error),
}
