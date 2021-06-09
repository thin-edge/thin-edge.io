use super::*;
use std::process::*;
use tedge_users::*;

pub struct SystemCommandRunner {
    pub user_manager: UserManager,
}

impl AbstractSystemCommandRunner for SystemCommandRunner {}

// We need this as `UserManager` is not `Debug`
impl std::fmt::Debug for SystemCommandRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SystemCommandRunner")
    }
}

impl RunSystemCommand<SystemdStopService> for SystemCommandRunner {
    fn run(
        &self,
        command: SystemdStopService,
    ) -> Result<SystemCommandExitStatus, SystemCommandError> {
        let _root_guard = self.user_manager.become_user(ROOT_USER);
        run_systemctl("stop", &command.service_name)
    }
}

impl RunSystemCommand<SystemdRestartService> for SystemCommandRunner {
    fn run(
        &self,
        command: SystemdRestartService,
    ) -> Result<SystemCommandExitStatus, SystemCommandError> {
        let _root_guard = self.user_manager.become_user(ROOT_USER);
        run_systemctl("restart", &command.service_name)
    }
}

impl RunSystemCommand<SystemdEnableService> for SystemCommandRunner {
    fn run(
        &self,
        command: SystemdEnableService,
    ) -> Result<SystemCommandExitStatus, SystemCommandError> {
        let _root_guard = self.user_manager.become_user(ROOT_USER);
        run_systemctl("enable", &command.service_name)
    }
}

impl RunSystemCommand<SystemdDisableService> for SystemCommandRunner {
    fn run(
        &self,
        command: SystemdDisableService,
    ) -> Result<SystemCommandExitStatus, SystemCommandError> {
        let _root_guard = self.user_manager.become_user(ROOT_USER);
        run_systemctl("disable", &command.service_name)
    }
}

impl RunSystemCommand<SystemdIsServiceActive> for SystemCommandRunner {
    fn run(
        &self,
        command: SystemdIsServiceActive,
    ) -> Result<SystemCommandExitStatus, SystemCommandError> {
        run_systemctl("is-active", &command.service_name)
    }
}

impl RunSystemCommand<SystemdVersion> for SystemCommandRunner {
    fn run(&self, _command: SystemdVersion) -> Result<SystemCommandExitStatus, SystemCommandError> {
        Command::new("systemctl")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(Into::into)
            .map_err(SystemCommandError::CommandExecutionFailed)
    }
}

fn run_systemctl(
    cmd: &str,
    service_name: &str,
) -> Result<SystemCommandExitStatus, SystemCommandError> {
    Command::new("systemctl")
        .arg(cmd)
        .arg(service_name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(Into::into)
        .map_err(SystemCommandError::CommandExecutionFailed)
}
