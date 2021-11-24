use std::ffi::OsStr;
use std::process::*;

/// A wrapper around `std::process::Command` to simplify command construction.
pub struct CommandBuilder {
    command: Command,
}

impl CommandBuilder {
    pub fn new(program: impl AsRef<OsStr>) -> CommandBuilder {
        Self {
            command: Command::new(program),
        }
    }

    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> CommandBuilder {
        self.command.arg(arg);
        self
    }

    pub fn silent(mut self) -> CommandBuilder {
        self.command.stdout(Stdio::null()).stderr(Stdio::null());
        self
    }

    pub fn build(self) -> Command {
        self.command
    }
}
