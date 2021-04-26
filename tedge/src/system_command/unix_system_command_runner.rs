use crate::system_command::*;
use std::os::unix::process::CommandExt;
use std::process::*;

pub struct UnixSystemCommandRunner;

impl SystemCommandRunner for UnixSystemCommandRunner {
    fn run(&self, system_command: SystemCommand) -> Result<ExitStatus, SystemCommandError> {
        into_command(system_command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(SystemCommandError::CommandExecutionFailed)
    }

    fn run_capturing_output(
        &self,
        system_command: SystemCommand,
    ) -> Result<std::process::Output, SystemCommandError> {
        into_command(system_command)
            .output()
            .map_err(SystemCommandError::CommandExecutionFailed)
    }
}

fn into_command(system_command: SystemCommand) -> Command {
    let SystemCommand {
        program,
        args,
        role,
    } = system_command;

    let mut command = Command::new(program);

    for arg in args {
        command.arg(arg);
    }

    if let Some(role) = role {
        assign_role(&mut command, &role);
    }

    command
}

fn assign_role(command: &mut Command, role: &Role) {
    let username = match role {
        Role::Root => "root",
        Role::TEdge => "tedge",
        Role::Broker => "mosquitto",
    };

    // XXX
    let user = users::get_user_by_name(username).unwrap();
    let group = users::get_group_by_name(username).unwrap();

    command.uid(user.uid());
    command.gid(group.gid());
}
