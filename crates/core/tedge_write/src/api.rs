//! Helpers for spawning `tedge-write` processes, to be used by other thin-edge components.

use anyhow::anyhow;
use anyhow::Context;
use camino::Utf8Path;
use std::process::Command;
use tedge_config::SudoCommandBuilder;

use crate::TEDGE_WRITE_BINARY;

/// Options for copying files using a `tedge-write` process.
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
        let mut command = self.command()?;

        let output = command.output();

        let program = command.get_program().to_string_lossy();
        let output = output.with_context(|| format!("failed to start process '{program}'"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            let err = match output.status.code() {
                Some(exit_code) => anyhow!(
                    "process '{program}' returned non-zero exit code ({exit_code}); stderr=\"{stderr}\""
                ),
                None => anyhow!("process '{program}' was terminated; stderr=\"{stderr}\""),
            };

            return Err(err);
        }

        Ok(())
    }

    fn command(&self) -> anyhow::Result<Command> {
        // if tedge-write is in PATH of tedge process, use it, if not, defer PATH lookup to sudo
        let tedge_write_binary =
            which::which_global(TEDGE_WRITE_BINARY).unwrap_or(TEDGE_WRITE_BINARY.into());

        let mut command = self.sudo.command(tedge_write_binary);

        let from_reader = std::fs::File::open(self.from)
            .with_context(|| format!("could not open file for reading '{}'", self.from))?;
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

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use super::*;

    const SUDO: &str = "sudo";

    #[test]
    fn uses_sudo_only_if_installed() {
        let temp_dir = tempfile::tempdir().unwrap();
        let source_path = temp_dir.path().join("source.txt");
        std::fs::File::create_new(&source_path).unwrap();

        let dest_path = temp_dir.path().join("destination");

        let options = CopyOptions {
            from: source_path.as_path().try_into().unwrap(),
            to: dest_path.as_path().try_into().unwrap(),
            sudo: SudoCommandBuilder::enabled(true),
            mode: None,
            user: None,
            group: None,
        };

        // when sudo not in path, start tedge-write without sudo
        std::env::set_var("PATH", temp_dir.path());
        let no_sudo_command = options.command().unwrap();
        assert_ne!(no_sudo_command.get_program(), SUDO);

        // if sudo is in path, start tedge-write with sudo
        let dummy_sudo_path = temp_dir.path().join(SUDO);
        let dummy_sudo = std::fs::File::create(dummy_sudo_path).unwrap();
        let mut dummy_sudo_permissions = dummy_sudo.metadata().unwrap().permissions();

        // chmod +x
        dummy_sudo_permissions.set_mode(dummy_sudo_permissions.mode() | 0o111);
        dummy_sudo.set_permissions(dummy_sudo_permissions).unwrap();

        let sudo_command = options.command().unwrap();
        // sudo can be either just program name or full path
        let sudo_command = Path::new(sudo_command.get_program());
        assert_eq!(sudo_command.file_name().unwrap(), SUDO);
    }
}
