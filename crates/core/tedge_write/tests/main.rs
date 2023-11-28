use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use assert_cmd::Command;
use tempfile::TempDir;

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
fn changes_file_permissions_if_file_doesnt_exist() {
    // Arrange
    let (temp_dir, source_path) = setup_source_file();
    fs::set_permissions(&source_path, Permissions::from_mode(0o644)).unwrap();

    let destination_path = temp_dir.path().join("destination.txt");

    let mut command = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    command.pipe_stdin(&source_path).unwrap();
    command.arg(&destination_path);

    command.args(["--mode", "600"]);

    // Act
    command.assert().success();

    // Assert
    let dest_mode = destination_path.metadata().unwrap().permissions().mode();
    // .mode() returns st_mode, we only need to compare a subset
    assert_eq!(dest_mode & 0o600, 0o600);
}

#[test]
fn doesnt_change_permissions_if_file_exists() {
    // Arrange
    let (temp_dir, source_path) = setup_source_file();
    fs::set_permissions(&source_path, Permissions::from_mode(0o644)).unwrap();
    let destination_path = temp_dir.path().join("destination.txt");

    fs::File::create(&destination_path).unwrap();

    let mut command = Command::cargo_bin(env!("CARGO_PKG_NAME")).unwrap();
    command.pipe_stdin(&source_path).unwrap();
    command.arg(&destination_path);

    command.args(["--mode", "600"]);

    // Act
    command.assert().success();

    // Assert
    let dest_mode = destination_path.metadata().unwrap().permissions().mode();
    assert_eq!(dest_mode & 0o644, 0o644);
}

fn setup_source_file() -> (TempDir, PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();

    let source_path = temp_dir.path().join("source.txt");
    let file_contents = "file contents";

    fs::write(&source_path, file_contents).unwrap();

    (temp_dir, source_path)
}
