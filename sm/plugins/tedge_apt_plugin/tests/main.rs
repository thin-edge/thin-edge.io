use hamcrest2::prelude::*;
use serial_test::serial;
use std::error;
use std::process::{Command, Stdio};
use std::sync::Once;
use tedge_utils::fs::atomically_write_file_sync;
use test_case::test_case;

type Result<T> = std::result::Result<T, Box<dyn error::Error>>;

const PACKAGE_NAME: &str = "rolldice";
const PACKAGE_VERSION: &str = "1.16-1+b3";
const TEDGE_APT_COMMAND: &str = "/etc/tedge/sm-plugins/apt";
const APT_COMMAND: &str = "/usr/bin/apt-get";
const PACKAGE_URL: &str =
    "http://ftp.br.debian.org/debian/pool/main/r/rolldice/rolldice_1.16-1+b3_amd64.deb";
const PACKAGE_FILE_PATH: &str = "/tmp/rolldice_1.16-1+b3_amd64.deb";
static DOWNLOAD_PACKAGE_BINARY: Once = Once::new();

pub fn download_package_binary_once() {
    DOWNLOAD_PACKAGE_BINARY.call_once_force(|_state| {
        simple_download(PACKAGE_URL);
    });
}

fn simple_download(url: &str) {
    let response = reqwest::blocking::get(url).unwrap();
    let content = response.bytes().unwrap();

    atomically_write_file_sync("/tmp/rolldice.deb", PACKAGE_FILE_PATH, content.as_ref()).unwrap();
}

/// converts a vector of u8 integers into utf8 String.
fn u8_to_string(vec: Vec<u8>) -> String {
    String::from_utf8(vec).unwrap()
}

/// executes a `cmd` with `args`
/// returns the stdout, stderr and exit code
fn run_cmd(cmd: &str, args: &str) -> Result<(String, String, i32)> {
    let args: Vec<&str> = args.split_whitespace().collect();
    let output = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    let stdout = u8_to_string(output.stdout);
    let stderr = u8_to_string(output.stderr);

    let status_code = output.status.code().unwrap();
    Ok((stdout, stderr, status_code))
}

#[test_case(
    &format!("install {} --file {}", PACKAGE_NAME, "wrong_path"),                                               // input
    "ERROR: Parsing Debian package failed",                                                                     // expected stderr
    5                                                                                                           // expected exit code
    ; "wrong path"                                                                                              // description
)]
#[test_case(
    &format!("install {} --file {} --module-version {}", PACKAGE_NAME, "not/a/package/path", PACKAGE_VERSION),  // input
    "ERROR: Parsing Debian package failed",                                                                     // expected stderr
    5                                                                                                           // expected exit code
    ; "wrong path with right version"                                                                           // description
)]
#[test_case(
    &format!("install {} --file {} --module-version {}", PACKAGE_NAME, PACKAGE_FILE_PATH, "some_version"),      // input
    "ERROR: Parsing Debian package failed",                                                                     // expected stderr
    5                                                                                                           // expected exit code
    ; "right path with wrong version"                                                                           // description
)]
#[test_case(
    &format!("install {} --file {} --module-version {}", PACKAGE_NAME, "not/a/package/path", "some_version"),   // input
    "ERROR: Parsing Debian package failed",                                                                     // expected stderr
    5                                                                                                           // expected exit code
    ; "wrong path with wrong version"                                                                           // description
)]
fn install_from_local_file_fail(
    input_command: &str,
    expected_stderr: &str,
    expected_exit_code: i32,
) -> Result<()> {
    // no setup needed, wrong arguments are provided to tedge apt plugin
    let (stdout, stderr, exit_code) = run_cmd(TEDGE_APT_COMMAND, input_command)?;

    // asserting command failed
    assert_that!(stdout.is_empty(), true);
    assert_that!(stderr, match_regex(expected_stderr));
    assert_that!(exit_code, eq(expected_exit_code));
    Ok(())
}

#[test_case(
    &format!("install {} --file {}", PACKAGE_NAME, PACKAGE_FILE_PATH),                                          // input
    &format!("The following NEW packages will be installed\n  {}", PACKAGE_NAME),                               // expected stdout
    0                                                                                                           // expected exit code
    ; "path"                                                                                                    // description
)]
#[serial]
#[test_case(
    &format!("install {} --file {} --module-version {}", PACKAGE_NAME, PACKAGE_FILE_PATH, PACKAGE_VERSION),     // input
    &format!("The following NEW packages will be installed\n  {}", PACKAGE_NAME),                               // expected stdout
    0                                                                                                           // expected exit code
    ; "path with version"                                                                                       // description
)]
#[serial]
#[test_case(
    &format!("install {} --module-version {} --file {}", PACKAGE_NAME,  PACKAGE_VERSION, PACKAGE_FILE_PATH),    // input
    &format!("The following NEW packages will be installed\n  {}", PACKAGE_NAME),                               // expected stdout
    0                                                                                                           // expected exit code
    ; "version with path"                                                                                       // description
)]
#[serial]
fn install_from_local_file_success(
    input_command: &str,
    expected_stdout: &str,
    expected_exit_code: i32,
) -> Result<()> {
    // fetching the debian package & removing rolldice in case it is already installed.
    // only executed once.
    download_package_binary_once();
    let _ = run_cmd(APT_COMMAND, &format!("remove {} -y", PACKAGE_NAME))?;

    // execute command to install from local file
    let (stdout, stderr, exit_code) = run_cmd(TEDGE_APT_COMMAND, input_command)?;

    // asserting success
    assert_that!(stdout, match_regex(expected_stdout));
    assert_that!(stderr.is_empty(), true);
    assert_that!(exit_code, eq(expected_exit_code));

    Ok(())
}
