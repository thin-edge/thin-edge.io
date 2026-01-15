use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tedge_test_utils::fs::TempTedgeDir;

#[test]
fn test_list_command() {
    let ttd = TempTedgeDir::new();
    let config_content = r#"
[[files]]
path = "/etc/tedge/tedge.toml"
type = "tedge.toml"

[[files]]
path = "/etc/app/config.json"
type = "app-config"
"#;
    ttd.dir("plugins")
        .file("tedge-configuration-plugin.toml")
        .with_raw_content(config_content);

    let mut cmd = Command::cargo_bin("tedge-file-config-plugin").unwrap();
    cmd.arg("--config-dir")
        .arg(ttd.path().to_str().unwrap())
        .arg("list");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("tedge.toml"))
        .stdout(predicate::str::contains("app-config"))
        .stdout(predicate::str::contains("tedge-configuration-plugin"));
}

#[test]
fn test_get_command_unsupported_type() {
    let ttd = TempTedgeDir::new();
    let config_content = r#"
[[files]]
path = "/etc/tedge/tedge.toml"
type = "tedge.toml"
"#;
    ttd.dir("plugins")
        .file("tedge-configuration-plugin.toml")
        .with_raw_content(config_content);

    let mut cmd = Command::cargo_bin("tedge-file-config-plugin").unwrap();
    cmd.arg("--config-dir")
        .arg(ttd.path().to_str().unwrap())
        .arg("get")
        .arg("unknown-type");

    cmd.assert().failure().stderr(predicate::str::contains(
        "not defined in the plugin configuration file",
    ));
}

#[test]
fn test_get_command_existing_file() {
    let ttd = TempTedgeDir::new();

    // Create a test config file
    let test_content = "test=value\nfoo=bar\n";
    let test_config_file = ttd.file("test.conf").with_raw_content(test_content);

    let config_content = format!(
        r#"
[[files]]
path = "{}"
type = "test.conf"
"#,
        test_config_file.path().display()
    );
    ttd.dir("plugins")
        .file("tedge-configuration-plugin.toml")
        .with_raw_content(&config_content);

    let mut cmd = Command::cargo_bin("tedge-file-config-plugin").unwrap();
    cmd.arg("--config-dir")
        .arg(ttd.path().to_str().unwrap())
        .arg("get")
        .arg("test.conf");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("test=value"))
        .stdout(predicate::str::contains("foo=bar"));
}

#[test]
fn test_set_command() {
    let ttd = TempTedgeDir::new();

    // Original config
    let config_content = "test=configuration\n";
    let dest_file = ttd.file("test.conf").with_raw_content(config_content);

    // New config
    let new_content = "new=configuration\n";
    let source_file = ttd.file("new.conf").with_raw_content(new_content);

    let config_content = format!(
        r#"
[[files]]
path = "{}"
type = "dest.conf"
"#,
        dest_file.utf8_path()
    );
    ttd.dir("plugins")
        .file("tedge-configuration-plugin.toml")
        .with_raw_content(&config_content);

    let mut cmd = Command::cargo_bin("tedge-file-config-plugin").unwrap();
    cmd.arg("--config-dir")
        .arg(ttd.path().to_str().unwrap())
        .arg("set")
        .arg("dest.conf")
        .arg(source_file.path().to_str().unwrap());

    cmd.assert().success();

    // Verify the file was copied
    let dest_content = fs::read_to_string(dest_file.path()).unwrap();
    assert_eq!(dest_content, new_content);
}

#[test]
fn test_set_command_restarts_service() {
    let ttd = TempTedgeDir::new();

    let witness_file = ttd.file("restart-witness.txt");

    // system.toml for restart command
    let system_toml_content = format!(
        r#"
[init]
name = "dummy"
is_available = ["true"]
restart = ["sh", "-c", "echo $0 >> {}", "{{}}"]
start = ["true"]
stop =  ["true"]
enable =  ["true"]
disable =  ["true"]
is_active = ["true"]
"#,
        witness_file.utf8_path()
    );
    ttd.file("system.toml")
        .with_raw_content(&system_toml_content);

    // Original config
    let dest_file = ttd.file("test.conf");

    // New config
    let new_content = "new=configuration\n";
    let source_file = ttd.file("new.conf").with_raw_content(new_content);

    let config_content = format!(
        r###"
[[files]]
path = "{}"
type = "dest.conf"
service = "dummy-service"
"###,
        dest_file.utf8_path()
    );
    ttd.dir("plugins")
        .file("tedge-configuration-plugin.toml")
        .with_raw_content(&config_content);

    let mut cmd = Command::cargo_bin("tedge-file-config-plugin").unwrap();
    cmd.arg("--config-dir")
        .arg(ttd.path().to_str().unwrap())
        .arg("set")
        .arg("dest.conf")
        .arg(source_file.path().to_str().unwrap());

    cmd.assert().success();

    // Verify that the restart command was called with the correct service name
    let witness_content = fs::read_to_string(witness_file.path()).unwrap();
    assert_eq!(witness_content.trim(), "dummy-service");
}

#[test]
fn test_empty_config_file() {
    let ttd = TempTedgeDir::new();

    ttd.dir("plugins")
        .file("tedge-configuration-plugin.toml")
        .with_raw_content("");

    let mut cmd = Command::cargo_bin("tedge-file-config-plugin").unwrap();
    cmd.arg("--config-dir")
        .arg(ttd.path().to_str().unwrap())
        .arg("list");

    // The default tedge-configuration-plugin type must still be listed
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("tedge-configuration-plugin"));
}

#[test]
fn test_invalid_config_file() {
    let ttd = TempTedgeDir::new();

    ttd.dir("plugins")
        .file("tedge-configuration-plugin.toml")
        .with_raw_content("not#toml");

    let mut cmd = Command::cargo_bin("tedge-file-config-plugin").unwrap();
    cmd.arg("--config-dir")
        .arg(ttd.path().to_str().unwrap())
        .arg("list");

    // The default tedge-configuration-plugin type must still be listed
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("tedge-configuration-plugin"));
}
