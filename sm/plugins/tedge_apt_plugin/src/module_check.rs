//use assert_cmd::prelude::*;
//use predicates::prelude::*;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

/// check that module_name is in file path
pub fn module_has_extension(file_path: &String) -> bool {
    let pb = PathBuf::from(file_path);
    let extension = pb.extension().unwrap();
    extension.to_str().unwrap() == "deb"
}

/// check that module_version is in file_path
pub fn module_has_version(module_version: &String, file_path: &String) -> bool {
    let pb = PathBuf::from(file_path);
    let file_name = pb.file_stem().unwrap();
    let file_version = file_name.to_str().unwrap().split('_').nth(1).unwrap();
    file_version == module_version
}

/// runs std command to query dpkg -i for `file_path`
fn get_module_metadata(file_path: &String) -> Result<Child, Box<dyn std::error::Error>> {
    let metadata = Command::new("dpkg")
        .arg("-I")
        .arg(&format!("{}", &file_path))
        .stdout(Stdio::piped())
        .spawn()?;
    Ok(metadata)
}

pub fn metadata_contains(
    file_path: &String,
    pattern: &String,
) -> Result<bool, Box<dyn std::error::Error>> {
    let metadata = get_module_metadata(&file_path)?;
    let metadata = String::from_utf8(metadata.wait_with_output()?.stdout)?;
    for line in metadata.lines() {
        if line.contains(pattern) {
            return Ok(true);
        }
    }
    Ok(false)
}
