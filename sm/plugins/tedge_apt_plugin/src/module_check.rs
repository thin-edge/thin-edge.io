use crate::error::InternalError;
use std::process::{Command, Stdio};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub struct PackageMetadata {
    file_path: PathBuf,
    metadata: Option<String>,
    remove_modified: bool,
}

impl PackageMetadata {
    pub fn try_new(file_path: &str) -> Result<Self, InternalError> {
        let metadata = String::from_utf8(Self::get_module_metadata(file_path)?)?;
        Ok(Self {
            file_path: PathBuf::from(file_path),
            metadata: Some(metadata),
            remove_modified: false,
        })
    }

    fn metadata_contains_all(&self, patterns: &[&str]) -> bool {
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

    fn get_module_metadata(file_path: &str) -> Result<Vec<u8>, InternalError> {
        Ok(Command::new("dpkg")
            .arg("-I")
            .arg(file_path)
            .stdout(Stdio::piped())
            .output()?
            .stdout)
    }

    pub fn validate_package(&mut self, contain_args: &[&str]) -> Result<(), InternalError> {
        if self.metadata_contains_all(contain_args) {
            dbg!(&self.file_path);
            if self.file_path.extension() != Some(OsStr::new("deb")) {
                let new_path = PathBuf::from(format!(
                    "{}.deb",
                    self.file_path().to_string_lossy().to_string()
                ));

                let _res = std::os::unix::fs::symlink(self.file_path(), &new_path);
                self.file_path = new_path;
                self.remove_modified = true;
            }

            Ok(())
        } else {
            Err(InternalError::ParsingError {
                file: self.file_path().to_string_lossy().to_string(),
            })
        }
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }
}

impl Drop for PackageMetadata {
    fn drop(&mut self) {
        if self.remove_modified {
            let _res = std::fs::remove_file(&self.file_path);
        }
    }
}
