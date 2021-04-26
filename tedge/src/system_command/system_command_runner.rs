use crate::system_command::*;

pub struct SystemCommandExitStatus(pub std::process::ExitStatus);

pub trait SystemCommandRunner {
    fn run(
        &self,
        system_command: SystemCommand,
    ) -> Result<SystemCommandExitStatus, SystemCommandError>;
}
