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
