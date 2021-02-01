use super::c8y::ConnectError;

type ExitCode = i32;

const MOSQUITTOCMD_SUCCESS: ExitCode = 3;
const SYSTEMCTL_ISACTIVE_SUCCESS: ExitCode = 3;
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
    IsActive,
    Restart,
    Status,
}

impl SystemCtlCmd {
    fn as_str(self) -> &'static str {
        match self {
            SystemCtlCmd::Cmd => "systemctl",
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

// This isn't complete way to retrieve HOME dir from the user.
// We could parse passwd file to get actual home path if we can get user name.
// I suppose rust provides some way to do it or allows through c bindings... But this implies unsafe code.
// Another alternative is to use deprecated env::home_dir() -1
// https://github.com/rust-lang/rust/issues/71684
pub fn home_dir() -> Option<std::path::PathBuf> {
    return std::env::var_os("HOME")
        .and_then(|home| if home.is_empty() { None } else { Some(home) })
        .map(std::path::PathBuf::from);
}

// Another simple method which has now been deprecated.
// (funny, advice says look on crates.io two of crates supposedly do what is expected are not necessarily correct:
// one uses unsafe code and another uses this method with deprecated env call)
pub fn home_dir2() -> Option<std::path::PathBuf> {
    #[allow(deprecated)]
    std::env::home_dir()
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

fn mosquitto_available() -> Result<(), ConnectError> {
    mosquitto_cmd_nullstdio(MosquittoParam::Status.as_str(), MOSQUITTOCMD_SUCCESS)
}

fn mosquitto_available_as_service() -> Result<(), ConnectError> {
    systemctl_cmd_nullstdio(
        SystemCtlCmd::Status.as_str(),
        MosquittoCmd::Cmd.as_str(),
        SYSTEMCTL_STATUS_SUCCESS,
    )
}

fn mosquitto_is_active_daemon() -> Result<(), ConnectError> {
    systemctl_is_active_nullstdio(MosquittoCmd::Cmd.as_str(), MOSQUITTOCMD_SUCCESS)
}

// Note that restarting a unit with this command does not necessarily flush out all of the unit's resources before it is started again.
// For example, the per-service file descriptor storage facility (see FileDescriptorStoreMax= in systemd.service(5)) will remain intact
// as long as the unit has a job pending, and is only cleared when the unit is fully stopped and no jobs are pending anymore.
// If it is intended that the file descriptor store is flushed out, too, during a restart operation an explicit
// systemctl stop command followed by systemctl start should be issued.
pub fn mosquitto_restart_daemon() -> Result<(), ConnectError> {
    match systemctl_restart_nullstdio(MosquittoCmd::Cmd.as_str(), SYSTEMCTL_ISACTIVE_SUCCESS) {
        Ok(_) => Ok(()),
        Err(_) => Err(ConnectError::MosquittoNotAvailableAsService),
    }
}

fn systemctl_is_active_nullstdio(service: &str, expected_code: i32) -> Result<(), ConnectError> {
    systemctl_cmd_nullstdio(SystemCtlCmd::IsActive.as_str(), service, expected_code)
}

fn systemctl_restart_nullstdio(service: &str, expected_code: i32) -> Result<(), ConnectError> {
    systemctl_cmd_nullstdio(SystemCtlCmd::Restart.as_str(), service, expected_code)
}

fn systemctl_cmd_nullstdio(
    cmd: &str,
    service: &str,
    expected_code: i32,
) -> Result<(), ConnectError> {
    match cmd_nullstdio_args(SystemCtlCmd::Cmd.as_str(), &[cmd, service], expected_code) {
        true => Ok(()),
        false => Err(ConnectError::SystemctlFailed { reason: "".into() }),
    }
}

fn mosquitto_cmd_nullstdio(cmd: &str, expected_code: i32) -> Result<(), ConnectError> {
    match cmd_nullstdio_args(MosquittoCmd::Cmd.as_str(), &[cmd], expected_code) {
        true => Ok(()),
        false => Err(ConnectError::MosquittoFailed { reason: "".into() }),
    }
}

fn cmd_nullstdio_args(command: &str, args: &[&str], expected_code: i32) -> bool {
    std::process::Command::new(command)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .map(|status| status.code() == Some(expected_code))
        .unwrap_or(false)
}

pub fn all_services_available() -> Result<(), ConnectError> {
    // This is just quick and naive way to check if systemd is available,
    // we should most likely find a better way to perform this check.
    systemd_available()?;

    // Check mosquitto exists on the system
    mosquitto_available()?;

    // Check mosquitto is available through systemd
    // Theoretically we could just do a big boom and run just this command as it will error on following:
    //  - systemd not available
    //  - mosquitto not installed as a service
    // That for instance would be sufficient and would return an error anyway, but I prefer to do it gently with separate checks.
    mosquitto_available_as_service()?;

    // Check mosquitto is running
    mosquitto_is_active_daemon()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmd_nullstdio_args_expected() {
        // There is a chance that this may fail on very embedded system which will not have 'ls' command on busybox.
        assert_eq!(cmd_nullstdio_args("ls", &[], 0), true);
        assert_eq!(cmd_nullstdio_args("test-command", &[], 0), false);
    }

    #[test]
    fn home_dir_test() {
        std::env::set_var("HOME", "/home/test/");
        let expected_path = std::path::PathBuf::from("/home/test/");
        assert_eq!(home_dir(), Some(expected_path));

        std::env::remove_var("HOME");
        assert_eq!(home_dir(), None);
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
