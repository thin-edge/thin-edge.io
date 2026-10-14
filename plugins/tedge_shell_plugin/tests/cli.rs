use assert_cmd::Command;
use predicates::str::contains;
use tedge_test_utils::fs::TempTedgeDir;

const BINARY_NAME: &str = "tedge-shell-plugin";

#[test]
fn a_subcommand_is_required() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");

    plugin(&config_dir)
        .args(["--command", "echo hello"])
        .assert()
        .failure();
}

#[test]
fn reports_the_command_output_as_a_workflow_script_output() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");

    execute_and_collect(&config_dir, "echo hello world")
        .success()
        .stdout(":::begin-tedge:::\n{\"result\":\"hello world\\n\"}\n:::end-tedge:::\n");
}

#[test]
fn accepts_a_command_starting_with_a_hyphen() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");

    execute_and_collect(&config_dir, "-no-such-command 2>/dev/null; echo ran")
        .success()
        .stdout(":::begin-tedge:::\n{\"result\":\"ran\\n\"}\n:::end-tedge:::\n");
}

#[test]
fn the_shell_is_read_from_the_tedge_config() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "[shell]\npath = \"/no/such/shell\"\n");

    execute_and_collect(&config_dir, "echo hello")
        .failure()
        .stdout(contains(
            r#""reason":"Failed to run the command using the shell '/no/such/shell'"#,
        ));
}

#[test]
fn the_timeout_is_read_from_the_tedge_config() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "[shell]\ntimeout = \"1s\"\n");

    execute_and_collect(&config_dir, "echo started; sleep 30")
        .code(124)
        .stdout(contains(r#""reason":"Command timed out after 1s"#));
}

#[test]
fn the_outcome_of_a_background_command_is_collected() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");

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
    with_config(&config_dir, "");

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
    with_config(&config_dir, "");

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
    with_config(&config_dir, "");

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
fn a_missing_data_dir_is_reported_with_its_path() {
    let config_dir = TempTedgeDir::new();
    let data_dir = config_dir.path().join("no-such-dir");
    config_dir
        .file("tedge.toml")
        .with_raw_content(&format!("[data]\npath = \"{data_dir}\"\n"));
    let reason = format!(r#""reason":"the configured data.path '{data_dir}' does not exist""#);

    plugin(&config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--command", "echo hello"])
        .assert()
        .failure();

    // The data dir is not created by the plugin
    assert!(!data_dir.exists());

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .failure()
        .stdout(contains(reason.as_str()));
}

#[test]
fn a_command_killed_after_setting_a_successful_result_is_successful() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");
    // The command kills the plugin running it, as a restart of the agent would,
    // finding its command id and the tedge config dir in its environment
    let command = format!(
        "echo before the restart; {} set-result --outcome successful && kill -9 $PPID",
        env!("CARGO_BIN_EXE_tedge-shell-plugin")
    );

    plugin(&config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--command", &command])
        .assert()
        .failure();

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .success()
        .stdout(contains(
            r#"{"result":"before the restart\n<the command has been interrupted, its output may be incomplete>\n","result_set":true}"#,
        ));
}

#[test]
fn a_command_killed_after_setting_a_failed_result_is_failed_with_its_reason() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");
    let command = format!(
        "{} set-result --outcome failed --reason 'Failed to do something' && kill -9 $PPID",
        env!("CARGO_BIN_EXE_tedge-shell-plugin")
    );

    plugin(&config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--command", &command])
        .assert()
        .failure();

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .code(1)
        .stdout(contains(
            r#"{"reason":"Failed to do something","result":"<the command has been interrupted, its output may be incomplete>\n","result_set":true}"#,
        ));
}

#[test]
fn an_inconsistent_result_is_rejected() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");

    for args in [
        &["--outcome", "successful", "--reason", "all good"][..],
        &["--outcome", "failed", "--exit-code", "3"],
    ] {
        plugin(&config_dir)
            .args(["set-result", "--cmd-id", "c8y-mapper-1234"])
            .args(args)
            .assert()
            .failure();
    }
}

#[test]
fn a_command_killed_without_expecting_its_interruption_is_failed() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");

    plugin(&config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--command", "kill -9 $PPID"])
        .assert()
        .failure();

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .failure()
        .stdout(contains("interrupted before completion"));
}

#[test]
fn a_background_command_is_given_its_id_without_tedge_config_warnings() {
    let config_dir = TempTedgeDir::new();
    with_config(&config_dir, "");
    // Nothing is interrupted, the command completing with its output reported
    let command = format!(
        "echo id=$SHELL_EXECUTE_CMD_ID; {} set-result --outcome successful 2>&1",
        env!("CARGO_BIN_EXE_tedge-shell-plugin")
    );

    plugin(&config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--command", &command])
        .assert()
        .success();

    plugin(&config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
        .success()
        .stdout(contains(r#"{"result":"id=c8y-mapper-1234\n"}"#));
}

fn plugin(config_dir: &TempTedgeDir) -> Command {
    let mut cmd = Command::cargo_bin(BINARY_NAME).unwrap();
    cmd.arg("--config-dir").arg(config_dir.path());
    cmd
}

/// Use the config dir as data dir, along with the given tedge config settings
fn with_config(config_dir: &TempTedgeDir, settings: &str) {
    // The job directories are owned by the user running the tests
    config_dir
        .file("system.toml")
        .with_raw_content("user = \"\"\ngroup = \"\"\n");
    config_dir.file("tedge.toml").with_raw_content(&format!(
        "[data]\npath = \"{}\"\n{settings}",
        config_dir.path()
    ));
}

/// Execute the command in the background, then collect its outcome
fn execute_and_collect(config_dir: &TempTedgeDir, command: &str) -> assert_cmd::assert::Assert {
    plugin(config_dir)
        .args(["execute", "--cmd-id", "c8y-mapper-1234"])
        .args(["--command", command])
        .timeout(std::time::Duration::from_secs(20))
        .assert()
        .success();

    plugin(config_dir)
        .args(["collect", "--cmd-id", "c8y-mapper-1234"])
        .assert()
}
