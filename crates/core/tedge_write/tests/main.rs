use std::fs;
use std::fs::Permissions;
use std::os::unix::fs::chown;
use std::os::unix::fs::MetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;

use assert_cmd::Command;
use tedge_config::SudoCommandBuilder;
use tempfile::TempDir;

const SUDO: &str = "sudo";

#[test]
fn creates_dest_file_if_doesnt_exist() {
    // Arrange
    let (temp_dir, source_path) = setup_source_file();
    let destination_path = temp_dir.path().join("destination.txt");

    let mut command = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    command.pipe_stdin(&source_path).unwrap();
    command.arg(&destination_path);

    // Act
    command.assert().success();

    // Assert
    assert_eq!(
        fs::read_to_string(source_path).unwrap(),
        fs::read_to_string(destination_path).unwrap(),
    );
}

#[test]
fn changes_permissions_if_destination_doesnt_exist() {
    // Arrange
    let (temp_dir, source_path) = setup_source_file();
    chown(&source_path, Some(2137), Some(2137)).unwrap();

    let destination_path = temp_dir.path().join("destination.txt");

    let mut command = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    command.pipe_stdin(&source_path).unwrap();
    command.arg(&destination_path);

    command.args(["--user", "2138", "--group", "2138", "--mode", "600"]);

    // Act
    command.assert().success();

    // Assert
    let dest_metadata = destination_path.metadata().unwrap();
    // .mode() returns st_mode, we only need to compare a subset
    let dest_mode = dest_metadata.permissions().mode() & 0o777;

    assert_eq!(dest_metadata.uid(), 2138);
    assert_eq!(dest_metadata.gid(), 2138);

    assert_eq!(dest_mode, 0o600);
}

#[test]
fn preserves_permissions_if_destination_exists() {
    // Arrange
    let (temp_dir, source_path) = setup_source_file();

    let destination_path = temp_dir.path().join("destination.txt");
    fs::File::create(&destination_path).unwrap();
    chown(&destination_path, Some(2137), Some(2137)).unwrap();
    fs::set_permissions(&destination_path, Permissions::from_mode(0o666)).unwrap();

    let mut command = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    command.pipe_stdin(&source_path).unwrap();
    command.arg(&destination_path);

    command.args(["--user", "2138", "--group", "2138", "--mode", "600"]);

    // Act
    command.assert().success();

    // Assert
    let dest_metadata = destination_path.metadata().unwrap();
    let dest_mode = dest_metadata.permissions().mode() & 0o777;

    assert_eq!(dest_metadata.uid(), 2137);
    assert_eq!(dest_metadata.gid(), 2137);
    assert_eq!(dest_mode, 0o666, "{dest_mode:o} != 666");
}

#[test]
fn uses_sudo_only_if_installed() {
    let (temp_dir, source_path) = setup_source_file();
    let dest_path = temp_dir.path().join("destination");
    std::env::set_var("PATH", temp_dir.path());

    let options = tedge_write::CopyOptions {
        from: source_path.as_path().try_into().unwrap(),
        to: dest_path.as_path().try_into().unwrap(),
        sudo: SudoCommandBuilder::enabled(true),
        mode: None,
        user: None,
        group: None,
    };

    let no_sudo_command = options.command().unwrap();
    assert_ne!(no_sudo_command.get_program(), SUDO);

    let dummy_sudo_path = temp_dir.path().join(SUDO);
    let dummy_sudo = std::fs::File::create(dummy_sudo_path).unwrap();
    let mut dummy_sudo_permissions = dummy_sudo.metadata().unwrap().permissions();
    // chmod +x
    dummy_sudo_permissions.set_mode(dummy_sudo_permissions.mode() | 0o111);
    dummy_sudo.set_permissions(dummy_sudo_permissions).unwrap();

    let sudo_command = options.command().unwrap();
    // sudo can be either just program name or full path
    let sudo_command = Path::new(sudo_command.get_program());
    assert_eq!(sudo_command.file_name().unwrap(), SUDO);
}

fn setup_source_file() -> (TempDir, PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();

    let source_path = temp_dir.path().join("source.txt");
    let file_contents = "file contents";

    fs::write(&source_path, file_contents).unwrap();

    (temp_dir, source_path)
}
