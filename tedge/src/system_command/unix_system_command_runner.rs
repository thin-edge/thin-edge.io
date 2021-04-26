use crate::system_command::*;
use std::os::unix::process::CommandExt;
use std::process::*;

pub struct UnixSystemCommandRunner;

impl SystemCommandRunner for UnixSystemCommandRunner {
    fn run(
        &self,
        system_command: SystemCommand,
    ) -> Result<SystemCommandExitStatus, SystemCommandError> {
        let mut command = into_command(system_command);
        let output = command
            .output()
            .map_err(SystemCommandError::CommandExecutionFailed)?;
        Ok(SystemCommandExitStatus(output.status))
    }
}

fn into_command(system_command: SystemCommand) -> Command {
    let SystemCommand {
        program,
        args,
        capture_output,
        capture_error,
        role,
        timeout,
    } = system_command;

    let mut command = Command::new(program);

    for arg in args {
        command.arg(arg);
    }

    match capture_output {
        None => {
            command.stdout(Stdio::null());
        }
        Some(_) => unimplemented!(),
    }

    match capture_error {
        None => {
            command.stderr(Stdio::null());
        }
        Some(_) => unimplemented!(),
    }

    if let Some(_) = timeout {
        unimplemented!()
    }

    if let Some(role) = role {
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

    command
}
