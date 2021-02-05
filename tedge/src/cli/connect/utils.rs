use std::{
    path::{Path, PathBuf},
    process::ExitStatus,
};

use which::which;

use super::c8y::ConnectError;

type ExitCode = i32;

const MOSQUITTOCMD_IS_ACTIVE: ExitCode = 130;
const MOSQUITTOCMD_SUCCESS: ExitCode = 3;
const SYSTEMCTL_SUCCESS: ExitCode = 0;
const SYSTEMCTL_STATUS_SUCCESS: ExitCode = 3;

enum MosquittoCmd {
    Cmd,
}

impl MosquittoCmd {
    fn as_str(self) -> &'static str {
        match self {
            MosquittoCmd::Cmd => "mosquitto",
        }
    }
}

enum MosquittoParam {
    Status,
}

impl MosquittoParam {
    fn as_str(self) -> &'static str {
        match self {
            MosquittoParam::Status => "-h",
        }
    }
}

enum SystemCtlCmd {
    Cmd,
    Enable,
    IsActive,
    Restart,
    Status,
}

impl SystemCtlCmd {
    fn as_str(self) -> &'static str {
        match self {
            SystemCtlCmd::Cmd => "systemctl",
            SystemCtlCmd::Enable => "enable",
            SystemCtlCmd::IsActive => "is-active",
            SystemCtlCmd::Restart => "restart",
            SystemCtlCmd::Status => "status",
        }
    }
}
enum SystemCtlParam {
    Version,
}

impl SystemCtlParam {
    fn as_str(self) -> &'static str {
        match self {
            SystemCtlParam::Version => "--version",
        }
    }
}

/// Check if systemd and mosquitto are available on the system.
pub fn all_services_available() -> Result<(), ConnectError> {
    systemd_available()
        .and_then(|()| mosquitto_available())
        .and_then(|()| mosquitto_available_as_service())
        .and_then(|()| mosquitto_is_active_daemon())
}

// Path util functions
pub fn build_path_from_home<T: AsRef<Path>>(paths: &[T]) -> Result<String, ConnectError> {
    build_path_from_home_as_path(paths).and_then(pathbuf_to_string)
}

fn build_path_from_home_as_path<T: AsRef<Path>>(paths: &[T]) -> Result<PathBuf, ConnectError> {
    let home_dir = home_dir().ok_or(ConnectError::ConfigurationExists)?;

    let mut final_path: PathBuf = PathBuf::from(home_dir);
    for path in paths {
        final_path.push(path);
    }
    Ok(final_path)
}

fn pathbuf_to_string(pathbuf: PathBuf) -> Result<String, ConnectError> {
    pathbuf
        .into_os_string()
        .into_string()
        .map_err(|_os_string| ConnectError::BridgeConnectionFailed)
}

fn sudo_path() -> Result<PathBuf, ConnectError> {
    Ok(which("sudo")?)
}

// This isn't complete way to retrieve HOME dir from the user.
// We could parse passwd file to get actual home path if we can get user name.
// I suppose rust provides some way to do it or allows through c bindings... But this implies unsafe code.
// Another alternative is to use deprecated env::home_dir() -1
// https://github.com/rust-lang/rust/issues/71684
fn home_dir() -> Option<PathBuf> {
    return std::env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        .map(PathBuf::from);
}

// Commands util functions
fn cmd_nullstdio_args(
    command: &str,
    args: &[&str],
    expected_code: i32,
) -> Result<bool, ConnectError> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_or_else(
            |err| Err(err),
            |status| Ok(status.code() == Some(expected_code)),
        )?)
}

fn cmd_nullstdio_args_with_code(command: &str, args: &[&str]) -> Result<ExitStatus, ConnectError> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?)
}

fn cmd_nullstdio_args_with_code_with_sudo(
    command: &str,
    args: &[&str],
) -> Result<ExitStatus, ConnectError> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?)
}

fn mosquitto_available() -> Result<(), ConnectError> {
    match cmd_nullstdio_args(
        MosquittoCmd::Cmd.as_str(),
        &[MosquittoParam::Status.as_str()],
        MOSQUITTOCMD_SUCCESS,
    ) {
        Ok(true) => Ok(()),
        Ok(false) => Err(ConnectError::MosquittoNotAvailable),
        Err(err) => Err(err),
    }
}

fn mosquitto_available_as_service() -> Result<(), ConnectError> {
    match cmd_nullstdio_args_with_code(
        SystemCtlCmd::Cmd.as_str(),
        &[SystemCtlCmd::Status.as_str(), MosquittoCmd::Cmd.as_str()],
    ) {
        Ok(status) => match status.code() {
            Some(SYSTEMCTL_STATUS_SUCCESS) | Some(SYSTEMCTL_SUCCESS) => Ok(()),
            Some(MOSQUITTOCMD_IS_ACTIVE) => Err(ConnectError::MosquittoNotAvailableAsService),
            code => Err(ConnectError::UnknownReturnCode { code }),
        },
        Err(err) => Err(err),
    }
}

fn mosquitto_is_active_daemon() -> Result<(), ConnectError> {
    match cmd_nullstdio_args_with_code(
        SystemCtlCmd::Cmd.as_str(),
        &[SystemCtlCmd::IsActive.as_str(), MosquittoCmd::Cmd.as_str()],
    ) {
        Ok(status) => match status.code() {
            Some(MOSQUITTOCMD_SUCCESS) | Some(SYSTEMCTL_SUCCESS) => Ok(()),
            Some(MOSQUITTOCMD_IS_ACTIVE) => Err(ConnectError::MosquittoIsActive),
            code => Err(ConnectError::UnknownReturnCode { code }),
        },
        Err(err) => Err(err),
    }
}

// Note that restarting a unit with this command does not necessarily flush out all of the unit's resources before it is started again.
// For example, the per-service file descriptor storage facility (see FileDescriptorStoreMax= in systemd.service(5)) will remain intact
// as long as the unit has a job pending, and is only cleared when the unit is fully stopped and no jobs are pending anymore.
// If it is intended that the file descriptor store is flushed out, too, during a restart operation an explicit
// systemctl stop command followed by systemctl start should be issued.
pub fn mosquitto_restart_daemon() -> Result<(), ConnectError> {
    let sudo = pathbuf_to_string(sudo_path()?)?;
    match cmd_nullstdio_args_with_code_with_sudo(
        sudo.as_str(),
        &[
            SystemCtlCmd::Cmd.as_str(),
            SystemCtlCmd::Restart.as_str(),
            MosquittoCmd::Cmd.as_str(),
        ],
    ) {
        Ok(status) => match status.code() {
            Some(MOSQUITTOCMD_SUCCESS) | Some(0) => Ok(()),
            Some(MOSQUITTOCMD_IS_ACTIVE) => Err(ConnectError::MosquittoCantPersist),
            code => Err(ConnectError::UnknownReturnCode { code }),
        },
        Err(err) => Err(err),
    }
}

pub fn mosquitto_enable_daemon() -> Result<(), ConnectError> {
    let sudo = pathbuf_to_string(sudo_path()?)?;
    match cmd_nullstdio_args_with_code_with_sudo(
        sudo.as_str(),
        &[
            SystemCtlCmd::Cmd.as_str(),
            SystemCtlCmd::Enable.as_str(),
            MosquittoCmd::Cmd.as_str(),
        ],
    ) {
        Ok(status) => match status.code() {
            Some(MOSQUITTOCMD_SUCCESS) | Some(0) => Ok(()),
            Some(MOSQUITTOCMD_IS_ACTIVE) => Err(ConnectError::MosquittoCantPersist),
            code => Err(ConnectError::UnknownReturnCode { code }),
        },
        Err(err) => Err(err),
    }
}

// How about using some crates like for example 'which'??
fn systemd_available() -> Result<(), ConnectError> {
    std::process::Command::new(SystemCtlCmd::Cmd.as_str())
        .arg(SystemCtlParam::Version.as_str())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_or_else(
            |_error| Err(ConnectError::SystemdNotAvailable),
            |status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(ConnectError::SystemdNotAvailable)
                }
            },
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[ignore]
    #[test]
    fn build_path_from_home_multiple_arguments() {
        let expected: &str = "/home/test/test/.test";
        std::env::set_var("HOME", "/home/test/");
        assert_eq!(build_path_from_home(&["test", ".test"]).unwrap(), expected);
    }

    #[test]
    fn cmd_nullstdio_args_expected() {
        // There is a chance that this may fail on very embedded system which will not have 'ls' command on busybox.
        assert_eq!(cmd_nullstdio_args("ls", &[], 0).unwrap(), true);

        match cmd_nullstdio_args("test-command", &[], 0) {
            Err(_err) => assert!(true),
            _ => assert!(false, "Error should be ConnectError"),
        }
    }

    #[ignore]
    #[test]
    fn home_dir_test() {
        let home = std::env::var("HOME").unwrap();
        std::env::set_var("HOME", "/home/test/");
        let expected_path = std::path::PathBuf::from("/home/test/");
        assert_eq!(home_dir(), Some(expected_path));

        std::env::remove_var("HOME");
        assert_eq!(home_dir(), None);
        std::env::set_var("HOME", home);
    }

    #[test]
    fn mosquitto_available_with_path() {
        if is_in_path("mosquitto") {
            assert!(mosquitto_available().is_ok());
        } else {
            assert!(mosquitto_available().is_err());
        }
    }

    fn is_in_path(command: &str) -> bool {
        if let Ok(path) = std::env::var("PATH") {
            for cmd in path.split(":") {
                let cmd_str = format!("{}/{}", cmd, command);
                if std::fs::metadata(cmd_str).is_ok() {
                    return true;
                }
            }
        }
        false
    }
}
