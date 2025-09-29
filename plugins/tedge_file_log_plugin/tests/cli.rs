use assert_cmd::Command;
use predicates::prelude::*;
use predicates::str::contains;
use std::fs;
use std::fs::File;
use std::io::Write;
use tedge_test_utils::fs::TempTedgeDir;
use time::OffsetDateTime;

const BINARY_NAME: &str = "tedge-file-log-plugin";

fn setup() -> (TempTedgeDir, String) {
    let temp_dir = TempTedgeDir::new();
    let temp_path = temp_dir.path().to_str().unwrap().to_string();

    // Create config directory structure
    let config_dir = temp_dir.dir("config");
    let plugins_dir = config_dir.dir("plugins");

    // Create app.log with mixed log levels
    let app_log = temp_dir.file("app.log");
    let mut file = File::create(app_log.path()).unwrap();
    writeln!(file, "INFO Application started").unwrap();
    writeln!(file, "DEBUG Loading configuration").unwrap();
    writeln!(file, "INFO Processing request").unwrap();
    writeln!(file, "ERROR Request timeout").unwrap();

    // Create service1.log with mixed log levels
    let service1_log = temp_dir.path().join("service1.log");
    let mut file = File::create(&service1_log).unwrap();
    writeln!(file, "INFO Service1 started").unwrap();
    writeln!(file, "DEBUG Service1 initializing").unwrap();
    writeln!(file, "ERROR Service1 database connection failed").unwrap();
    writeln!(file, "INFO Service1 ready").unwrap();

    // Create service2.log with mixed log levels
    let service2_log = temp_dir.path().join("service2.log");
    let mut file = File::create(&service2_log).unwrap();
    writeln!(file, "INFO Service2 started").unwrap();
    writeln!(file, "DEBUG Service2 initializing").unwrap();
    writeln!(file, "ERROR Service2 database connection failed").unwrap();
    writeln!(file, "INFO Service2 ready").unwrap();

    // Create plugin config file with 2 entries
    let config_content = format!(
        r#"
[[files]]
path = "{}/app.log"
type = "app"

[[files]]
path = "{}/service*.log"
type = "service"
"#,
        temp_path, temp_path
    );

    let plugin_config_file = plugins_dir.file("tedge-log-plugin.toml");
    fs::write(plugin_config_file.path(), config_content).unwrap();

    (temp_dir, config_dir.utf8_path().to_string())
}

#[test]
fn list_command() {
    let (_temp_dir, config_dir) = setup();

    let mut cmd = Command::cargo_bin(BINARY_NAME).unwrap();
    cmd.args(["--config-dir", &config_dir])
        .arg("list")
        .assert()
        .success()
        .stdout(contains("app"))
        .stdout(contains("service"));
}

#[test]
fn get_command_basic() {
    let (_temp_dir, config_dir) = setup();

    let mut cmd = Command::cargo_bin(BINARY_NAME).unwrap();
    cmd.args(["--config-dir", &config_dir])
        .args(["get", "app"])
        .assert()
        .success()
        .stdout(contains("INFO Application started"))
        .stdout(contains("DEBUG Loading configuration"))
        .stdout(contains("INFO Processing request"))
        .stdout(contains("ERROR Request timeout"));
}

#[test]
fn get_command_since_and_until_parse_unix_timestamp() {
    let (_temp_dir, config_dir) = setup();

    let now = OffsetDateTime::now_utc().unix_timestamp();
    let since = now - 10;
    let until = now + 10;
    let mut cmd = Command::cargo_bin(BINARY_NAME).unwrap();
    cmd.args(["--config-dir", &config_dir])
        .args([
            "get",
            "app",
            "--since",
            since.to_string().as_str(),
            "--until",
            until.to_string().as_str(),
        ])
        .assert()
        .success();
}

#[test]
fn get_command_with_tail() {
    let (_temp_dir, config_dir) = setup();

    let mut cmd = Command::cargo_bin(BINARY_NAME).unwrap();
    cmd.args(["--config-dir", &config_dir])
        .args(["get", "app", "--tail", "2"])
        .assert()
        .success()
        .stdout(contains("INFO Processing request"))
        .stdout(contains("ERROR Request timeout"))
        .stdout(contains("DEBUG Loading configuration").not());
}

#[test]
fn get_command_nonexistent_log_type() {
    let (_temp_dir, config_dir) = setup();

    let mut cmd = Command::cargo_bin(BINARY_NAME).unwrap();
    cmd.args(["--config-dir", &config_dir])
        .args(["get", "nonexistent"])
        .assert()
        .failure()
        .stderr(contains("No logs found for log type"));
}
