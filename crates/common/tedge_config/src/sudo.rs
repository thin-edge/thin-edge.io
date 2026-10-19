use camino::Utf8PathBuf;
use std::ffi::OsStr;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;
use tedge_api::script::ShellScript;
use tracing::warn;

use crate::TEdgeConfig;

const SUDO: &str = "sudo";

/// The sudo option to fail instead of prompting for a password
const NON_INTERACTIVE: &str = "-n";

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

    /// Decides whether a command has to be prepended by `sudo` or other prefix.
    ///
    /// Checks if sudo is present in $PATH, unless sudo is disabled.
    pub fn resolve(&self) -> SudoResolution {
        if !self.enabled {
            return SudoResolution::Disabled;
        }

        match which::which(self.sudo_program.as_ref()).map(Utf8PathBuf::from_path_buf) {
            Ok(Ok(sudo)) => SudoResolution::Sudo(sudo),
            _ => SudoResolution::NotFound,
        }
    }

    /// Creates a command, optionally prepended by `sudo` or other prefix.
    ///
    /// Checks if sudo is present in $PATH. If it's present and sudo is enabled,
    /// prepare a [`Command`](std::process::Command) using sudo. Otherwise,
    /// prepares a Command that starts `program` directly.
    pub fn command<S: AsRef<OsStr>>(&self, program: S) -> Command {
        let program = program.as_ref();
        let resolution = self.resolve();
        match &resolution {
            SudoResolution::Sudo(sudo) => {
                let mut c = Command::new(sudo);
                c.arg(NON_INTERACTIVE);
                c.arg(program);
                c
            }
            SudoResolution::Disabled | SudoResolution::NotFound => {
                if let Some(warning) = resolution.warning(&program.to_string_lossy()) {
                    warn!("{warning}");
                }
                Command::new(program)
            }
        }
    }

    /// Prepends a script with `sudo -n --` or other prefix, if sudo is enabled and present in $PATH
    ///
    /// The resolution is returned along with the script to launch,
    /// so the caller can report a missing sudo where it sees fit.
    ///
    /// The prefix is added as separate arguments, so the arguments of the script cannot be split
    /// into extra sudo arguments. The `--` stops sudo from reading the script command
    /// as an option or as a `VAR=value` environment assignment.
    pub fn script(&self, script: ShellScript) -> (ShellScript, SudoResolution) {
        let resolution = self.resolve();
        let script = match &resolution {
            SudoResolution::Sudo(sudo) => {
                let mut args = vec![
                    NON_INTERACTIVE.to_string(),
                    "--".to_string(),
                    script.command,
                ];
                args.extend(script.args);
                ShellScript {
                    command: sudo.to_string(),
                    args,
                }
            }
            SudoResolution::Disabled | SudoResolution::NotFound => script,
        };
        (script, resolution)
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
                        if self.is_authorized(program, args) {
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
    /// This is done by running `sudo -n --list -- <command> <args>`.
    pub fn is_authorized<S: AsRef<OsStr>>(&self, program: &impl AsRef<OsStr>, args: &[S]) -> bool {
        let SudoResolution::Sudo(sudo) = self.resolve() else {
            return false;
        };
        let status = Command::new(sudo)
            .arg(NON_INTERACTIVE)
            .arg("--list")
            .arg("--")
            .arg(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        matches!(status, Ok(status) if status.success())
    }
}

/// How a command has to be launched according to the user's sudo preference
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum SudoResolution {
    /// Prepend the command with this sudo program and `-n`
    Sudo(Utf8PathBuf),

    /// `sudo.enable` is false: run the command directly
    Disabled,

    /// `sudo.enable` is true, but no sudo program was found in $PATH: run the command directly
    NotFound,
}

impl SudoResolution {
    /// The warning to report when `program` is run directly although sudo is enabled
    pub fn warning(&self, program: &str) -> Option<String> {
        match self {
            SudoResolution::NotFound => Some(format!(
                "`sudo.enable` set to `true`, but sudo not found in $PATH, invoking '{program}' directly"
            )),
            SudoResolution::Sudo(_) | SudoResolution::Disabled => None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sudo_is_not_used_when_disabled() {
        let sudo = SudoCommandBuilder::enabled(false);
        assert_eq!(sudo.resolve(), SudoResolution::Disabled);
    }

    #[test]
    fn sudo_is_used_when_enabled_and_found() {
        let sudo = SudoCommandBuilder::with_program("echo");
        let echo = Utf8PathBuf::from_path_buf(which::which("echo").unwrap()).unwrap();
        assert_eq!(sudo.resolve(), SudoResolution::Sudo(echo));
    }

    #[test]
    fn sudo_is_not_used_when_enabled_but_not_found() {
        let sudo = SudoCommandBuilder::with_program("no-such-sudo-program");
        assert_eq!(sudo.resolve(), SudoResolution::NotFound);
    }

    #[test]
    fn a_command_is_authorized_when_sudo_lists_it() {
        let sudo = SudoCommandBuilder::with_program("true");
        assert!(sudo.is_authorized(&"tedge", &["service", "restart", "nginx"]));
    }

    #[test]
    fn a_command_is_not_authorized_when_sudo_does_not_list_it() {
        let sudo = SudoCommandBuilder::with_program("false");
        assert!(!sudo.is_authorized(&"tedge", &["service", "restart", "nginx"]));
    }

    #[test]
    fn a_command_is_not_authorized_when_sudo_is_disabled() {
        let sudo = SudoCommandBuilder::enabled(false);
        assert!(!sudo.is_authorized(&"tedge", &["service", "restart", "nginx"]));
    }

    #[test]
    fn an_installed_sudo_prefixes_the_script() {
        let sudo = SudoCommandBuilder::with_program("echo");
        let echo = Utf8PathBuf::from_path_buf(which::which("echo").unwrap()).unwrap();
        let script: ShellScript = "tedge service restart nginx".parse().unwrap();

        let (elevated, resolution) = sudo.script(script);

        assert_eq!(resolution, SudoResolution::Sudo(echo.clone()));
        assert_eq!(
            elevated.to_string(),
            format!("{echo} -n -- tedge service restart nginx")
        );
    }

    #[test]
    fn the_sudo_prefix_is_kept_apart_from_the_script_arguments() {
        let sudo = SudoCommandBuilder::with_program("echo");
        let script = ShellScript {
            command: "/some/script.sh".to_string(),
            args: vec!["-u root; reboot".to_string(), "a b".to_string()],
        };

        let (elevated, _) = sudo.script(script);

        assert_eq!(
            elevated.args,
            vec!["-n", "--", "/some/script.sh", "-u root; reboot", "a b"]
        );
    }

    #[test]
    fn the_script_is_run_directly_when_sudo_is_disabled() {
        let sudo = SudoCommandBuilder::enabled(false);
        let script: ShellScript = "tedge service restart nginx".parse().unwrap();

        let (launched, resolution) = sudo.script(script.clone());

        assert_eq!(launched, script);
        assert_eq!(resolution.warning("tedge"), None);
    }

    #[test]
    fn the_script_is_run_directly_with_a_warning_when_sudo_is_not_found() {
        let sudo = SudoCommandBuilder::with_program("no-such-sudo-program");
        let script: ShellScript = "tedge service restart nginx".parse().unwrap();

        let (launched, resolution) = sudo.script(script.clone());

        assert_eq!(launched, script);
        assert_eq!(
            resolution.warning("tedge").as_deref(),
            Some("`sudo.enable` set to `true`, but sudo not found in $PATH, invoking 'tedge' directly")
        );
    }
}
