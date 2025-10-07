use std::ffi::OsStr;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use tracing::warn;

use crate::TEdgeConfig;

const SUDO: &str = "sudo";

/// A object used to spawn processes according to the user's sudo preference.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SudoCommandBuilder {
    enabled: bool,
    sudo_program: Arc<str>,
}

impl SudoCommandBuilder {
    /// Configures the object to prepend `sudo` if the `sudo.enable` config
    /// setting is enabled.
    pub fn new(config: &TEdgeConfig) -> Self {
        Self::enabled(config.sudo.enable)
    }

    /// Configures the object to always prepend `sudo`.
    pub fn enabled(enabled: bool) -> Self {
        Self {
            enabled,
            sudo_program: Arc::from(SUDO),
        }
    }

    /// Instead of `sudo`, configures object to prepend other program name.
    ///
    /// Mainly used by tests that don't wish to actually execute the command as
    /// `sudo` would, so they replace it with e.g. `echo`.
    pub fn with_program(program: impl Into<Arc<str>>) -> Self {
        Self {
            enabled: true,
            sudo_program: program.into(),
        }
    }

    /// Creates a command, optionally prepended by `sudo` or other prefix.
    ///
    /// Checks if sudo is present in $PATH. If it's present and sudo is enabled,
    /// prepare a [`Command`](std::process::Command) using sudo. Otherwise,
    /// prepares a Command that starts `program` directly.
    pub fn command<S: AsRef<OsStr>>(&self, program: S) -> Command {
        let program = program.as_ref();
        if !self.enabled {
            return Command::new(program);
        }

        match which::which(self.sudo_program.as_ref()) {
            Ok(sudo) => {
                let mut c = Command::new(sudo);
                // non-interactive
                c.arg("-n");
                c.arg(program);
                c
            }
            Err(_) => {
                warn!("`sudo.enable` set to `true`, but sudo not found in $PATH, invoking '{}' directly", program.to_string_lossy());
                Command::new(program)
            }
        }
    }

    /// Ensure the command can be executed using sudo
    ///
    /// Be warned, that the command is actually executed.
    pub fn ensure_command_succeeds<S: AsRef<OsStr>>(
        &self,
        program: &impl AsRef<OsStr>,
        args: &Vec<S>,
    ) -> Result<(), SudoError> {
        let output = self
            .command(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output();
        match output {
            Ok(output) if output.status.success() => Ok(()),
            Ok(output) => {
                tracing::error!(target: "sudo", "{} failed with stderr: <<EOF\n{}\nEOF",
                    program.as_ref().to_string_lossy(),
                    String::from_utf8_lossy(output.stderr.as_ref()));
                match output.status.code() {
                    Some(exit_code) => {
                        if self.command_is_sudo_enabled(program, args) {
                            Err(SudoError::ExecutionFailed(exit_code))
                        } else {
                            Err(SudoError::CannotSudo)
                        }
                    }
                    None => Err(SudoError::ExecutionInterrupted),
                }
            }
            Err(err) => Err(SudoError::CannotExecute(err)),
        }
    }

    /// Check that sudo is enabled and the user authorized to run the command with sudo
    ///
    /// This is done by running `sudo --list <command> <args>`.
    fn command_is_sudo_enabled<S: AsRef<OsStr>>(
        &self,
        program: &impl AsRef<OsStr>,
        args: &Vec<S>,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        let Ok(sudo) = which::which(self.sudo_program.as_ref()) else {
            return false;
        };
        let status = Command::new(sudo)
            .arg("-n")
            .arg("--list")
            .arg(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        matches!(status, Ok(status) if status.success())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum SudoError {
    #[error("The user has not been authorized to run the command with sudo")]
    CannotSudo,

    #[error(transparent)]
    CannotExecute(#[from] std::io::Error),

    #[error("The command returned a non-zero exit code: {0}")]
    ExecutionFailed(i32),

    #[error("The command has been interrupted by a signal")]
    ExecutionInterrupted,
}
