use std::ffi::OsStr;
use std::process::Command;
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
                c.arg(program);
                c
            }
            Err(_) => {
                warn!("`sudo.enable` set to `true`, but sudo not found in $PATH, invoking '{}' directly", program.to_string_lossy());
                Command::new(program)
            }
        }
    }
}
