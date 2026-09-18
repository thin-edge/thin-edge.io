use assert_cmd::Command;
use predicates::str::contains;
use tedge_test_utils::fs::TempTedgeDir;

const BINARY_NAME: &str = "tedge-shell-plugin";

fn plugin(config_dir: &TempTedgeDir) -> Command {
    let mut cmd = Command::cargo_bin(BINARY_NAME).unwrap();
    cmd.arg("--config-dir").arg(config_dir.path());
    cmd
}

#[test]
fn reports_the_command_output_as_a_workflow_script_output() {
    let config_dir = TempTedgeDir::new();
    plugin(&config_dir)
        .args(["--command", "echo hello world"])
        .assert()
        .success()
        .stdout(":::begin-tedge:::\n{\"result\":\"hello world\\n\"}\n:::end-tedge:::\n");
}

#[test]
fn accepts_a_command_starting_with_a_hyphen() {
    let config_dir = TempTedgeDir::new();
    plugin(&config_dir)
        .args(["--command", "-no-such-command 2>/dev/null; echo ran"])
        .assert()
        .success()
        .stdout(":::begin-tedge:::\n{\"result\":\"ran\\n\"}\n:::end-tedge:::\n");
}

#[test]
fn propagates_the_command_exit_code() {
    let config_dir = TempTedgeDir::new();
    plugin(&config_dir)
        .args(["--command", "echo oops >&2; exit 7"])
        .assert()
        .code(7)
        .stdout(contains(r#""reason":"Command returned exit code 7: oops""#))
        .stdout(contains(r#""result":"oops\n""#));
}

#[test]
fn the_shell_can_be_overridden_from_the_command_line() {
    let config_dir = TempTedgeDir::new();
    plugin(&config_dir)
        .args(["--shell", "/no/such/shell", "--command", "echo hello"])
        .assert()
        .failure()
        // The reason is reported using the script output protocol,
        // so the workflow tells the user why the command could not be run
        .stdout(contains(
            r#""reason":"Failed to run the command using the shell '/no/such/shell'"#,
        ))
        .stderr(contains("/no/such/shell"));
}

#[test]
fn the_shell_is_read_from_the_tedge_config() {
    let config_dir = TempTedgeDir::new();
    config_dir
        .file("tedge.toml")
        .with_raw_content("[shell]\npath = \"/no/such/shell\"\n");

    plugin(&config_dir)
        .args(["--command", "echo hello"])
        .assert()
        .failure()
        .stderr(contains("/no/such/shell"));
}

#[test]
fn the_timeout_is_read_from_the_tedge_config() {
    let config_dir = TempTedgeDir::new();
    config_dir
        .file("tedge.toml")
        .with_raw_content("[shell]\ntimeout = \"1s\"\n");

    plugin(&config_dir)
        .args(["--command", "echo started; sleep 30"])
        .timeout(std::time::Duration::from_secs(20))
        .assert()
        .code(124)
        .stdout(contains(r#""reason":"Command timed out after 1s"#));
}

fn with_tmp_dir(config_dir: &TempTedgeDir) {
    config_dir
        .file("tedge.toml")
        .with_raw_content(&format!("[tmp]\npath = \"{}\"\n", config_dir.path()));
}

#[test]
fn the_outcome_of_a_background_command_is_collected() {
    let config_dir = TempTedgeDir::new();
    with_tmp_dir(&config_dir);

    plugin(&config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--command", "echo oops; exit 3"])
        .assert()
        .success()
        .stdout("");

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .code(3)
        .stdout(contains(r#""reason":"Command returned exit code 3: oops""#))
        .stdout(contains(r#""result":"oops\n""#));
}

#[test]
fn a_launch_error_of_a_background_command_is_collected() {
    let config_dir = TempTedgeDir::new();
    with_tmp_dir(&config_dir);

    plugin(&config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--shell", "/no/such/shell", "--command", "echo hello"])
        .assert()
        .success();

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .failure()
        .stdout(contains(
            r#""reason":"Failed to run the command using the shell '/no/such/shell'"#,
        ));
}

#[test]
fn an_interrupted_command_is_failed() {
    let config_dir = TempTedgeDir::new();
    with_tmp_dir(&config_dir);

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .failure()
        .stdout(contains(
            r#""reason":"The command was interrupted before completion, most likely by a restart of tedge-agent or of the device""#,
        ));
}

#[test]
fn an_invalid_command_id_is_rejected() {
    let config_dir = TempTedgeDir::new();
    with_tmp_dir(&config_dir);

    plugin(&config_dir)
        .args([
            "execute",
            "--cmd-id",
            "../escape",
            "--command",
            "echo hello",
        ])
        .assert()
        .failure()
        .stderr(contains("Invalid command id"));
}

#[test]
fn a_missing_tmp_dir_is_reported_with_its_path() {
    let config_dir = TempTedgeDir::new();
    let tmp_dir = config_dir.path().join("no-such-dir");
    config_dir
        .file("tedge.toml")
        .with_raw_content(&format!("[tmp]\npath = \"{}\"\n", tmp_dir));
    let reason = format!(
        r#""reason":"the configured tmp.path '{}' does not exist""#,
        tmp_dir
    );

    plugin(&config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--command", "echo hello"])
        .assert()
        .failure();

    // The tmp dir is not created by the plugin
    assert!(!tmp_dir.exists());

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .failure()
        .stdout(contains(reason.as_str()));

    plugin(&config_dir)
        .args(["--command", "echo hello"])
        .assert()
        .failure()
        .stdout(contains(reason.as_str()));
}
