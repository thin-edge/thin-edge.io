use crate::system_command::*;

pub trait SystemCommandRunner {
    /// Runs `system_command` without capturing any output.
    fn run(
        &self,
        system_command: SystemCommand,
    ) -> Result<std::process::ExitStatus, SystemCommandError>;

    /// Runs `system_command` with capturing stdout and stderr output.
    fn run_capturing_output(
        &self,
        system_command: SystemCommand,
    ) -> Result<std::process::Output, SystemCommandError>;
}
