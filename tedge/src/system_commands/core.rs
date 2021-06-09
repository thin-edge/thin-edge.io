use std::process::ExitStatus;

pub trait SystemCommand {}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct SystemCommandExitStatus(ExitStatus);

impl From<ExitStatus> for SystemCommandExitStatus {
    fn from(exit_status: ExitStatus) -> Self {
        Self(exit_status)
    }
}

impl SystemCommandExitStatus {
    pub fn success(&self) -> bool {
        self.0.success()
    }
    pub fn code(&self) -> Option<i32> {
        self.0.code()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SystemCommandError {
    #[error("Insufficient permissions for running command.")]
    InsufficientPermissions,

    #[error("Failed to execute command")]
    CommandExecutionFailed(std::io::Error),
}

pub trait RunSystemCommand<T: SystemCommand> {
    fn run(&self, command: T) -> Result<SystemCommandExitStatus, SystemCommandError>;
}
