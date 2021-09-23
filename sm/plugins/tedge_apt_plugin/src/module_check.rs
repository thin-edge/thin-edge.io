//use assert_cmd::prelude::*;
//use predicates::prelude::*;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// check that module_name is in file path
pub fn filepath_has_extension(file_path: &str) -> bool {
    let pb = PathBuf::from(file_path);
    let extension = pb.extension().unwrap();
    extension.to_str().unwrap() == "deb"
}

pub fn set_filepath_extension(file_path: &str) -> String {
    file_path.to_owned() + ".deb"
}

pub struct PackageMetadata {
    metadata: Option<String>,
}

impl PackageMetadata {
    pub fn try_new(file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let metadata = String::from_utf8(Self::get_module_metadata(file_path)?)?;
        Ok(Self {
            metadata: Some(metadata),
        })
    }

    pub fn metadata_contains_all(&self, patterns: &[&str]) -> bool {
        for pattern in patterns {
            if !self.metadata_contains(pattern) {
                return false;
            }
        }
        true
    }

    fn metadata_contains(&self, pattern: &str) -> bool {
        if let Some(lines) = &self.metadata {
            return lines.contains(pattern);
        }
        false
    }

    fn get_module_metadata(file_path: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        Ok(Command::new("dpkg")
            .arg("-I")
            .arg(&format!("{}", &file_path))
            .stdout(Stdio::piped())
            .output()?
            .stdout)
    }
}
