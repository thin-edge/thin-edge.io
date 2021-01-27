use std::env;
use std::path::PathBuf;

use super::c8y::ConnectError;

enum MosquittoCmd {
    Base,
    Status,
}

impl MosquittoCmd {
    fn as_str(self) -> &'static str {
        match self {
            MosquittoCmd::Base => "mosquitto",
            MosquittoCmd::Status => "-h",
        }
    }
}

enum SystemCtlCmd {
    Base,
    IsActive,
    Restart,
    Status,
    Version,
}

impl SystemCtlCmd {
    fn as_str(self) -> &'static str {
        match self {
            SystemCtlCmd::Base => "systemctl",
            SystemCtlCmd::IsActive => "is-active",
            SystemCtlCmd::Restart => "restart",
            SystemCtlCmd::Status => "status",
            SystemCtlCmd::Version => "--version",
        }
    }
}

type ExitCode = i32;
enum ExitCodes {}

impl ExitCodes {
    pub const MOSQUITTOCMD_SUCCESS: ExitCode = 3;
    pub const SUCCESS: ExitCode = 0;
    pub const SYSTEMCTL_ISACTICE_SUCCESS: ExitCode = 3;
    pub const SYSTEMCTL_STATUS_SUCCESS: ExitCode = 3;
}

// This isn't complete way to retrieve HOME dir from the user.
// We could parse passwd file to get actual home path if we can get user name.
// I suppose rust provides some way to do it or allows through c bindings... But this implies unsafe code.
// Another alternative is to use deprecated env::home_dir() -1
// https://github.com/rust-lang/rust/issues/71684
pub fn home_dir() -> Option<PathBuf> {
    return env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        // .or_else(|| return None; )
        .map(PathBuf::from);
}

// Another simple method which has now been deprecated.
// (funny, advice says look on crates.io two of crates supposedly do what is expected are not necessarily correct:
// one uses unsafe code and another uses this method with deprecated env call)
pub fn home_dir2() -> Option<PathBuf> {
    #[allow(deprecated)]
    std::env::home_dir()
}

// How about using some crates like for example 'which'??
fn systemd_available() -> Result<bool, ConnectError> {
    std::process::Command::new(SystemCtlCmd::Base.as_str())
        .arg(SystemCtlCmd::Version.as_str())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_or_else(
            |_| Err(ConnectError::SystemdUnavailable),
            |status| Ok(status.success()),
        )
}

fn mosquitto_available() -> Result<bool, ConnectError> {
    match mosquitto_cmd_nostd(MosquittoCmd::Status.as_str(), 3) {
        true => Ok(true),
        false => Err(ConnectError::MosquittoNotAvailable),
    }
}

fn mosquitto_available_as_service() -> Result<bool, ConnectError> {
    match systemctl_cmd_nostd(
        SystemCtlCmd::Status.as_str(),
        MosquittoCmd::Base.as_str(),
        ExitCodes::SYSTEMCTL_STATUS_SUCCESS,
    ) {
        true => Ok(true),
        false => Err(ConnectError::MosquittoNotAvailableAsService),
    }
}

fn mosquitto_is_active_daemon() -> Result<bool, ConnectError> {
    systemctl_is_active_nostd(MosquittoCmd::Base.as_str(), ExitCodes::MOSQUITTOCMD_SUCCESS)
}

// Note that restarting a unit with this command does not necessarily flush out all of the unit's resources before it is started again.
// For example, the per-service file descriptor storage facility (see FileDescriptorStoreMax= in systemd.service(5)) will remain intact
// as long as the unit has a job pending, and is only cleared when the unit is fully stopped and no jobs are pending anymore.
// If it is intended that the file descriptor store is flushed out, too, during a restart operation an explicit
// systemctl stop command followed by systemctl start should be issued.
pub fn mosquitto_restart_daemon() -> Result<(), ConnectError> {
    match systemctl_restart_nostd(
        MosquittoCmd::Base.as_str(),
        ExitCodes::SYSTEMCTL_ISACTICE_SUCCESS,
    ) {
        Ok(_) => Ok(()),
        Err(_) => Err(ConnectError::MosquittoNotAvailableAsService),
    }
}

fn systemctl_is_active_nostd(service: &str, expected_code: i32) -> Result<bool, ConnectError> {
    match systemctl_cmd_nostd(SystemCtlCmd::IsActive.as_str(), service, expected_code) {
        true => Ok(true),
        false => Err(ConnectError::SystemctlFailed {
            reason: format!("Service '{}' is not active", service).into(),
        }),
    }
}

fn systemctl_restart_nostd(service: &str, expected_code: i32) -> Result<bool, ConnectError> {
    match systemctl_cmd_nostd(SystemCtlCmd::Restart.as_str(), service, expected_code) {
        true => Ok(true),
        false => Err(ConnectError::SystemctlFailed {
            reason: "Restart required service {service}".into(),
        }),
    }
}

fn systemctl_cmd_nostd(cmd: &str, service: &str, expected_code: i32) -> bool {
    std::process::Command::new(SystemCtlCmd::Base.as_str())
        .arg(cmd)
        .arg(service)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .map(|status| status.code() == Some(expected_code))
        .unwrap_or(false)
}

fn mosquitto_cmd_nostd(cmd: &str, expected_code: i32) -> bool {
    std::process::Command::new(MosquittoCmd::Base.as_str())
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .map(|status| status.code() == Some(expected_code))
        .unwrap_or(false)
}

pub fn all_services_availble() -> Result<bool, ConnectError> {
    // This is just quick and naive way to check if systemd is available,
    // we should most likely find a better way to perform this check.
    systemd_available()?;

    // Check mosquitto exists on the system
    mosquitto_available()?;

    // Check mosquitto mosquitto available through systemd
    // Theoretically we could just do a big boom and run just this command as it will error on following:
    //  - systemd not available
    //  - mosquitto not installed as a service
    // That for instance would be sufficient and would return an error anyway, but I prefer to do it gently with separate checks.
    mosquitto_available_as_service()?;

    // Check mosquitto is running
    mosquitto_is_active_daemon()?;
    Ok(true)
}
