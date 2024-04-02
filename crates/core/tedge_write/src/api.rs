use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use std::process::Command;
use tedge_config::SudoCommandBuilder;

/// Additional flags passed to `tedge-write` process
#[derive(Debug, PartialEq)]
pub struct CopyOptions<'a> {
    /// Source path
    pub from: &'a Utf8Path,

    /// Destination path
    pub to: &'a Utf8Path,

    /// User's sudo preference, received from TedgeConfig
    pub sudo: SudoCommandBuilder,

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
        let mut command = self.sudo.command(crate::TEDGE_WRITE_PATH);

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
