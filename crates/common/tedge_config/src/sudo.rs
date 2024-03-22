use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use tracing::warn;

use crate::TEdgeConfig;

const SUDO: &str = "sudo";

/// An object used to remember the user's sudo preference.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SudoCommandBuilder {
    path: Option<Arc<Path>>,
    enabled: bool,
}

impl SudoCommandBuilder {
    pub fn new(config: &TEdgeConfig) -> Self {
        Self::enabled(config.sudo.enable)
    }

    pub fn enabled(enabled: bool) -> Self {
        let path = match which::which(SUDO) {
            Ok(sudo) => Some(Arc::from(sudo)),
            Err(_) => None,
        };

        Self { path, enabled }
    }

    /// Checks if sudo is present in $PATH. If it's present and sudo is enabled,
    /// prepare a [`Command`](std::process::Command) using sudo. Otherwise,
    /// prepares a Command that starts `program` directly.
    pub fn command<S: AsRef<OsStr>>(&self, program: S) -> Command {
        if !self.enabled {
            return Command::new(program);
        }

        match &self.path {
            Some(sudo) => {
                let mut c = Command::new(sudo.as_ref());
                c.arg(program);
                c
            }
            None => {
                warn!("`sudo.enable` set to `true`, but sudo not found in $PATH");
                Command::new(program)
            }
        }
    }
}
