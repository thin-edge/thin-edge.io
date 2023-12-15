use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use std::process::Command;

/// Additional flags passed to `tedge-write` process
#[derive(Debug, PartialEq)]
pub struct CopyOptions<'a> {
    /// Source path
    pub from: &'a Utf8Path,

    /// Destination path
    pub to: &'a Utf8Path,

    /// If tedge-write will be used with sudo
    pub sudo: bool,

    /// Permission mode for the file, in octal form.
    pub mode: Option<u32>,

    /// User which will become the new owner of the file.
    pub user: Option<&'a str>,

    /// Group which will become the new owner of the file.
    pub group: Option<&'a str>,
}

impl<'a> CopyOptions<'a> {
    /// Copies the file by spawning new tedge-write process.
    ///
    /// Stdin and Stdout are UTF-8.
    pub fn copy(self) -> anyhow::Result<()> {
        let output = self
            .command()?
            .output()
            .context("Starting tedge-write process failed")?;

        if !output.status.success() {
            return Err(anyhow!(
                String::from_utf8(output.stderr).expect("output should be utf-8")
            ));
        }

        Ok(())
    }

    pub fn command(&self) -> std::io::Result<Command> {
        let is_sudo_installed = which::which_global("sudo").is_ok();

        let mut command = if is_sudo_installed && self.sudo {
            let mut command = Command::new("sudo");
            command.arg(crate::TEDGE_WRITE_PATH);
            command
        } else {
            Command::new(crate::TEDGE_WRITE_PATH)
        };

        let from_reader = std::fs::File::open(self.from)?;
        command.stdin(from_reader).arg(self.to);

        if let Some(mode) = self.mode {
            command.arg("--mode").arg(format!("{mode:o}"));
        }
        if let Some(user) = self.user {
            command.arg("--user").arg(user);
        }
        if let Some(group) = self.group {
            command.arg("--group").arg(group);
        }

        Ok(command)
    }
}
