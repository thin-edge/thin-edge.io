use crate::error::InternalError;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// check that module_name is in file path
pub fn filepath_has_extension(file_path: &str) -> bool {
    let pb = PathBuf::from(file_path);
    match pb.extension() {
        Some(extension) => extension == "deb",
        None => false,
    }
}

pub struct PackageMetadata {
    metadata: Option<String>,
}

impl PackageMetadata {
    pub fn try_new(file_path: &str) -> Result<Self, InternalError> {
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

    fn get_module_metadata(file_path: &str) -> Result<Vec<u8>, InternalError> {
        Ok(Command::new("dpkg")
            .arg("-I")
            .arg(file_path)
            .stdout(Stdio::piped())
            .output()?
            .stdout)
    }
}

pub fn validate_package(file_path: &str, contain_args: &[&str]) -> Result<String, InternalError> {
    let package_metadata = PackageMetadata::try_new(file_path).unwrap();

    if package_metadata.metadata_contains_all(contain_args) {
        if !filepath_has_extension(file_path) {
            let file_path = create_link_with_extension(file_path)?;
            return Ok(file_path);
        }
        Ok(file_path.into())
    } else {
        Err(InternalError::ParsingError {
            file: file_path.to_string(),
        })
    }
}

fn create_link_with_extension(file_path: &str) -> Result<String, InternalError> {
    let link_path = format!("{}.deb", &file_path);
    std::os::unix::fs::symlink(file_path, &link_path)?;
    Ok(link_path)
}
