use std::io::{BufRead, BufReader, Read, Write};

#[derive(Debug)]
pub enum PrivilegedCommand {
    Command1,
}

#[derive(Debug, thiserror::Error)]
pub enum PrivilegedCommandError {
    #[error("Insufficient privileges: Try running with `sudo`")]
    InsufficientPrivileges,

    #[error("Command execution failed")]
    CommandFailed,
}

pub trait PrivilegedCommandExecutor {
    fn execute(&mut self, command: PrivilegedCommand) -> Result<(), PrivilegedCommandError>;
}

/// We use this executor when not running as `root`. It will always fail with an error.
pub struct UnprivilegedDummyCommandExecutor;

impl PrivilegedCommandExecutor for UnprivilegedDummyCommandExecutor {
    fn execute(&mut self, command: PrivilegedCommand) -> Result<(), PrivilegedCommandError> {
        eprintln!(
            "Trying to execute a privileged command as non-root: {:?}",
            command
        );
        Err(PrivilegedCommandError::InsufficientPrivileges)
    }
}

impl UnprivilegedDummyCommandExecutor {
    pub fn new() -> Self {
        // assert!(users::get_current_uid() != 0);
        Self {}
    }
}

/// When `tedge` is started as `root`, spawn `tedge_priv` which runs all code that
/// need `root` privileges. Immediately drop privileges for the rest of `tedge`.
///
pub struct PrivilegeSeparatedCommandExecutor {
    child: std::process::Child,
    child_stdin: std::process::ChildStdin,
    child_stdout: BufReader<std::process::ChildStdout>,
}

impl PrivilegeSeparatedCommandExecutor {
    pub fn new(path_to_tedge_priv: &str) -> Self {
        //assert!(users::get_current_uid() == 0);

        let mut child = std::process::Command::new(path_to_tedge_priv)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .expect("Failed to spawn tedge_priv");

        // TODO: Switch to unprivileged user tedge/tedge
        //
        // users::switch::switch_user_group("tedge", "tedge")

        let child_stdin = child.stdin.take().unwrap();
        let child_stdout = BufReader::new(child.stdout.take().unwrap());

        Self {
            child,
            child_stdin,
            child_stdout,
        }
    }
}

impl PrivilegedCommandExecutor for PrivilegeSeparatedCommandExecutor {
    fn execute(&mut self, command: PrivilegedCommand) -> Result<(), PrivilegedCommandError> {
        match command {
            PrivilegedCommand::Command1 => {
                self.child_stdin.write_all(b"command1\n").unwrap();
                self.child_stdin.flush().unwrap();
                let mut result = String::new();
                self.child_stdout.read_line(&mut result).unwrap();

                match result.trim() {
                    "OK" => Ok(()),
                    _ => Err(PrivilegedCommandError::CommandFailed),
                }
            }
        }
    }
}
