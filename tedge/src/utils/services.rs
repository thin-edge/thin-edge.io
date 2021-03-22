use std::process::ExitStatus;
use which::which;

use super::paths;

#[derive(thiserror::Error, Debug)]
pub enum ServicesError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("Couldn't set mosquitto server to start on boot.")]
    MosquittoCantPersist,

    #[error("Stop mosquitto service before you use this command: 'systemctl stop mosquitto'")]
    MosquittoIsActive,

    #[error("mosquitto is not installed on the system. Install mosquitto to use this command.")]
    MosquittoNotAvailable,

    #[error("mosquitto is installed but the related systemd-service is missing.")]
    MosquittoNotAvailableAsService,

    #[error(transparent)]
    PathsError(#[from] paths::PathsError),

    #[error("Couldn't find path to 'sudo'. Update $PATH variable with 'sudo' path.\n{0}")]
    SudoNotFound(#[from] which::Error),

    #[error(
        "Systemd is not available on the system or elevated permissions have not been granted."
    )]
    SystemdNotAvailable,

    #[error("Unexpected value for exit status.")]
    UnexpectedExitStatus,

    #[error("Returned exit code: '{code:?}' for: '{command}' is unhandled.")]
    UnhandledReturnCode { code: i32, command: String },
}

type ExitCode = i32;

const MOSQUITTOCMD_IS_ACTIVE: ExitCode = 130;
const MOSQUITTOCMD_SUCCESS: ExitCode = 3;
const SYSTEMCTL_SERVICE_RUNNING: ExitCode = 0;
const SYSTEMCTL_SUCCESS: ExitCode = 0;
const SYSTEMCTL_STATUS_SUCCESS: ExitCode = 3;

/// Check if systemd and mosquitto are available on the system.
pub fn all_services_available() -> Result<(), ServicesError> {
    systemd_available()
        .and_then(|()| mosquitto_available())
        .and_then(|()| mosquitto_available_as_service())
        .and_then(|()| mosquitto_is_active_daemon())
}

pub fn check_mosquitto_is_running() -> Result<bool, ServicesError> {
    let status = cmd_nullstdio_args_with_code(
        SystemCtlCmd::Cmd.as_str(),
        &[SystemCtlCmd::IsActive.as_str(), MosquittoCmd::Cmd.as_str()],
    )?;

    match status.code() {
        Some(SYSTEMCTL_SERVICE_RUNNING) => Ok(true),
        _ => Ok(false),
    }
}

// Note that restarting a unit with this command does not necessarily flush out all of the unit's resources before it is started again.
// For example, the per-service file descriptor storage facility (see FileDescriptorStoreMax= in systemd.service(5)) will remain intact
// as long as the unit has a job pending, and is only cleared when the unit is fully stopped and no jobs are pending anymore.
// If it is intended that the file descriptor store is flushed out, too, during a restart operation an explicit
// systemctl stop command followed by systemctl start should be issued.
pub fn mosquitto_restart_daemon() -> Result<(), ServicesError>{
    mosquitto_systemctl_daemon(SystemCtlCmd::Restart, ServicesError::MosquittoCantPersist)
}

pub fn mosquitto_enable_daemon() -> Result<(), ServicesError>{
    mosquitto_systemctl_daemon(SystemCtlCmd::Enable, ServicesError::MosquittoCantPersist)
}

fn mosquitto_available_as_service() -> Result<(), ServicesError> {
    mosquitto_systemctl_daemon(SystemCtlCmd::Status, ServicesError::MosquittoNotAvailableAsService)
}

fn mosquitto_is_active_daemon() -> Result<(), ServicesError> {
    mosquitto_systemctl_daemon(SystemCtlCmd::IsActive, ServicesError::MosquittoIsActive)
}

fn mosquitto_systemctl_daemon(systemctl_cmd: SystemCtlCmd, services_error: ServicesError) -> Result<(), ServicesError> {
    let sudo = paths::pathbuf_to_string(which("sudo")?)?;
    match cmd_nullstdio_args_with_code_with_sudo(
        sudo.as_str(),
        &[
            SystemCtlCmd::Cmd.as_str(),
            systemctl_cmd.as_str(),
            MosquittoCmd::Cmd.as_str(),
        ],
    ) {
        Ok(status) => match status.code() {
            Some(MOSQUITTOCMD_SUCCESS) | Some(SYSTEMCTL_SUCCESS) => Ok(()),
            Some(MOSQUITTOCMD_IS_ACTIVE) => Err(services_error),
            code => {
                let code = code.ok_or(ServicesError::UnexpectedExitStatus)?;
                Err(ServicesError::UnhandledReturnCode {
                    code,
                    command: SystemCtlCmd::Cmd.into(),
                })
            }
        },
        Err(err) => Err(err),
    }
}

pub fn tedge_mapper_start_daemon() -> Result<(), ServicesError> {
    tedge_mapper_systemctl_daemon(SystemCtlCmd::Start)
}

pub fn tedge_mapper_stop_daemon() -> Result<(), ServicesError> {
    tedge_mapper_systemctl_daemon(SystemCtlCmd::Stop)
}

pub fn tedge_mapper_enable_daemon() -> Result<(), ServicesError> {
    tedge_mapper_systemctl_daemon(SystemCtlCmd::Enable)
}

pub fn tedge_mapper_disable_daemon() -> Result<(), ServicesError> {
    tedge_mapper_systemctl_daemon(SystemCtlCmd::Disable)
}

fn tedge_mapper_systemctl_daemon(systemctl_cmd: SystemCtlCmd) -> Result<(), ServicesError> {
    let sudo = paths::pathbuf_to_string(which("sudo")?)?;
    match cmd_nullstdio_args_with_code_with_sudo(
        sudo.as_str(),
        &[
            SystemCtlCmd::Cmd.as_str(),
            systemctl_cmd.as_str(),
            TedgeMapperCmd::Cmd.as_str(),
        ],
    ) {
        Ok(status) => match status.code() {
            Some(SYSTEMCTL_SUCCESS) => Ok(()),
            code => {
                let code = code.ok_or(ServicesError::UnexpectedExitStatus)?;
                Err(ServicesError::UnhandledReturnCode {
                    code,
                    command: SystemCtlCmd::Cmd.into(),
                })
            }
        },
        Err(err) => Err(err),
    }
}

// Generic util functions
pub fn ok_if_not_found(err: std::io::Error) -> std::io::Result<()> {
    match err.kind() {
        std::io::ErrorKind::NotFound => Ok(()),
        _ => Err(err),
    }
}

// Commands util functions
fn cmd_nullstdio_args(
    command: &str,
    args: &[&str],
    expected_code: i32,
) -> Result<bool, ServicesError> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_or_else(Err, |status| Ok(status.code() == Some(expected_code)))?)
}

fn cmd_nullstdio_args_with_code(command: &str, args: &[&str]) -> Result<ExitStatus, ServicesError> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?)
}

fn cmd_nullstdio_args_with_code_with_sudo(
    command: &str,
    args: &[&str],
) -> Result<ExitStatus, ServicesError> {
    Ok(std::process::Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()?)
}

fn mosquitto_available() -> Result<(), ServicesError> {
    match cmd_nullstdio_args(
        MosquittoCmd::Cmd.as_str(),
        &[MosquittoParam::Status.as_str()],
        MOSQUITTOCMD_SUCCESS,
    ) {
        Ok(true) => Ok(()),
        Ok(false) => Err(ServicesError::MosquittoNotAvailable),
        Err(err) => Err(err),
    }
}

fn systemd_available() -> Result<(), ServicesError> {
    std::process::Command::new(SystemCtlCmd::Cmd.as_str())
        .arg(SystemCtlParam::Version.as_str())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_or_else(
            |_error| Err(ServicesError::SystemdNotAvailable),
            |status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(ServicesError::SystemdNotAvailable)
                }
            },
        )
}

#[derive(Debug)]
enum TedgeMapperCmd {
    Cmd,
}

impl TedgeMapperCmd {
    fn as_str(&self) -> &'static str {
        match self {
            TedgeMapperCmd::Cmd => "tedge-mapper",
        }
    }
}

impl Into<String> for TedgeMapperCmd {
    fn into(self) -> String {
        match self {
            TedgeMapperCmd::Cmd => "tedge-mapper".to_owned(),
        }
    }
}

#[derive(Debug)]
enum MosquittoCmd {
    Cmd,
}

impl MosquittoCmd {
    fn as_str(&self) -> &'static str {
        match self {
            MosquittoCmd::Cmd => "mosquitto",
        }
    }
}

impl Into<String> for MosquittoCmd {
    fn into(self) -> String {
        match self {
            MosquittoCmd::Cmd => "mosquitto".to_owned(),
        }
    }
}

#[derive(Debug)]
enum MosquittoParam {
    Status,
}

impl MosquittoParam {
    fn as_str(&self) -> &'static str {
        match self {
            MosquittoParam::Status => "-h",
        }
    }
}

impl Into<String> for MosquittoParam {
    fn into(self) -> String {
        match self {
            MosquittoParam::Status => "-h".to_owned(),
        }
    }
}

#[derive(Debug)]
enum SystemCtlCmd {
    Cmd,
    Enable,
    Disable,
    IsActive,
    Start,
    Stop,
    Restart,
    Status,
}

impl SystemCtlCmd {
    fn as_str(&self) -> &'static str {
        match self {
            SystemCtlCmd::Cmd => "systemctl",
            SystemCtlCmd::Enable => "enable",
            SystemCtlCmd::Disable => "disable",
            SystemCtlCmd::IsActive => "is-active",
            SystemCtlCmd::Start => "start",
            SystemCtlCmd::Stop => "stop",
            SystemCtlCmd::Restart => "restart",
            SystemCtlCmd::Status => "status",
        }
    }
}

impl Into<String> for SystemCtlCmd {
    fn into(self) -> String {
        match self {
            SystemCtlCmd::Cmd => "systemctl".to_owned(),
            SystemCtlCmd::Enable => "enable".to_owned(),
            SystemCtlCmd::Disable => "disable".to_owned(),
            SystemCtlCmd::IsActive => "is-active".to_owned(),
            SystemCtlCmd::Start => "start".to_owned(),
            SystemCtlCmd::Stop => "stop".to_owned(),
            SystemCtlCmd::Restart => "restart".to_owned(),
            SystemCtlCmd::Status => "status".to_owned(),
        }
    }
}

#[derive(Debug)]
enum SystemCtlParam {
    Version,
}

impl SystemCtlParam {
    fn as_str(&self) -> &'static str {
        match self {
            SystemCtlParam::Version => "--version",
        }
    }
}

impl Into<String> for SystemCtlParam {
    fn into(self) -> String {
        match self {
            SystemCtlParam::Version => "--version".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn cmd_nullstdio_args_expected() {
        // There is a chance that this may fail on very embedded system which will not have 'ls' command on busybox.
        assert_eq!(cmd_nullstdio_args("ls", &[], 0).unwrap(), true);

        if let Err(_err) = cmd_nullstdio_args("test-command", &[], 0) {
            panic!()
        }
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
            for cmd in path.split(':') {
                let cmd_str = format!("{}/{}", cmd, command);
                if std::fs::metadata(cmd_str).is_ok() {
                    return true;
                }
            }
        }
        false
    }
}
