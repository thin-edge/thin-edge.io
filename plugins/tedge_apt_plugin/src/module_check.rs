use crate::error::InternalError;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

pub struct PackageMetadata {
    file_path: PathBuf,
    metadata: String,
    remove_modified: bool,
}

impl PackageMetadata {
    pub fn try_new(file_path: &str) -> Result<Self, InternalError> {
        let metadata = String::from_utf8(Self::get_module_metadata(file_path)?)?;

        Ok(Self {
            file_path: PathBuf::from(file_path),
            metadata,
            remove_modified: false,
        })
    }

    fn metadata_contains_all(&self, patterns: &[&str]) -> Result<(), InternalError> {
        for pattern in patterns {
            if !&self.metadata.contains(pattern) {
                let given_metadata_split: Vec<&str> = pattern.split(':').collect();
                // Extract the expected meta data value for the given key
                // For example Package name, Version etc.
                let expected_metadata_split: Vec<&str> = self
                    .metadata
                    .split(&given_metadata_split[0])
                    .collect::<Vec<&str>>()[1]
                    .split('\n')
                    .collect::<Vec<&str>>()[0]
                    .split(':')
                    .collect();

                return Err(InternalError::MetaDataMismatch {
                    package: self.file_path().to_string_lossy().to_string(),
                    expected_key: given_metadata_split[0].to_string(),
                    expected_value: expected_metadata_split[1].to_string(),
                    provided_value: given_metadata_split[1].to_string(),
                });
            }
        }
        Ok(())
    }

    fn get_module_metadata(file_path: &str) -> Result<Vec<u8>, InternalError> {
        let res = Command::new("dpkg").arg("-I").arg(file_path).output()?;
        match res.status.success() {
            true => Ok(res.stdout),
            false => Err(InternalError::ParsingError {
                file: file_path.to_string(),
                error: String::from_utf8_lossy(&res.stderr).to_string(),
            }),
        }
    }

    pub fn validate_package(&mut self, contain_args: &[&str]) -> Result<(), InternalError> {
        self.metadata_contains_all(contain_args)?;
        // In the current implementation using `apt-get` it is required that the file has '.deb' extension (if we use dpkg extension doesn't matter).
        if self.file_path.extension() != Some(OsStr::new("deb")) {
            let new_path = PathBuf::from(format!("{}.deb", self.file_path().to_string_lossy()));

            let _res = std::os::unix::fs::symlink(self.file_path(), &new_path);
            self.file_path = new_path;
            self.remove_modified = true;
        }

        Ok(())
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
