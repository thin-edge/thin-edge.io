use std::ffi::OsStr;
use std::process::Command;
use tracing::warn;

use crate::TEdgeConfig;

const SUDO: &str = "sudo";

/// A object used to spawn processes according to the user's sudo preference.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct SudoCommandBuilder {
    enabled: bool,
}

impl SudoCommandBuilder {
    pub fn new(config: &TEdgeConfig) -> Self {
        Self::enabled(config.sudo.enable)
    }

    pub fn enabled(enabled: bool) -> Self {
        Self { enabled }
    }

    /// Checks if sudo is present in $PATH. If it's present and sudo is enabled,
    /// prepare a [`Command`](std::process::Command) using sudo. Otherwise,
    /// prepares a Command that starts `program` directly.
    pub fn command<S: AsRef<OsStr>>(&self, program: S) -> Command {
        if !self.enabled {
            return Command::new(program);
        }

        match which::which(SUDO) {
            Ok(sudo) => {
                let mut c = Command::new(sudo);
                c.arg(program);
                c
            }
            Err(_) => {
                warn!("`sudo.enable` set to `true`, but sudo not found in $PATH");
                Command::new(program)
            }
        }
    }
}
