//use assert_cmd::prelude::*;
//use predicates::prelude::*;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

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
    pub fn new() -> Self {
        Self { metadata: None }
    }

    pub fn try_new(mut self, file_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let () = self.get_module_metadata(file_path)?;
        Ok(self)
    }

    pub fn metadata_contains_all(&self, patterns: &[&str]) -> bool {
        for pattern in patterns {
            if !self.metadata_contains(pattern) {
                return false;
            }
        }
        true
    }

    pub fn metadata_contains(&self, pattern: &str) -> bool {
        if let Some(lines) = &self.metadata {
            return lines.contains(pattern);
        }
        false
    }

    fn get_module_metadata(&mut self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let metadata = Command::new("dpkg")
            .arg("-I")
            .arg(&format!("{}", &file_path))
            .stdout(Stdio::piped())
            .output()?
            .stdout;
        self.metadata = Some(String::from_utf8(metadata)?);
        Ok(())
    }
}
