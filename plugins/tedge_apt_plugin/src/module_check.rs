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
                // Note: Debian version may have an optional epoch prefix in the version,
                // e.g. "Version: 1:2.3.4-1", so limit max splitting to 2 fields
                let given_metadata_split: Vec<&str> = pattern.splitn(2, ':').collect();
                // Extract the expected meta data value for the given key
                // For example Package name, Version etc.
                let expected_metadata_split: Vec<&str> = self
                    .metadata
                    .split(&given_metadata_split[0])
                    .collect::<Vec<&str>>()[1]
                    .split('\n')
                    .collect::<Vec<&str>>()[0]
                    .splitn(2, ':')
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messsage_contains_correct_values_when_version_has_epoch_prefix() {
        // Modified output from `dpkg -I tedge.deb`
        let contents = r#"
new Debian package, version 2.0.
 size 6093866 bytes: control archive=2710 bytes.
       1 bytes,     0 lines      conffiles
     500 bytes,    17 lines      control
     116 bytes,     2 lines      md5sums
    2834 bytes,    60 lines   *  postinst             #!/bin/sh
    2257 bytes,   100 lines   *  postrm               #!/bin/sh
    3481 bytes,   115 lines   *  preinst              #!/bin/sh
      17 bytes,     2 lines   *  prerm                #!/bin/sh
 Package: tedge
 Version: 1:1.5.1
 Section: misc
 Priority: optional
 Architecture: arm64
 License: Apache-2.0
 Maintainer: thin-edge.io team <info@thin-edge.io>
 Installed-Size: 13196
 Suggests: mosquitto
 Homepage: https://thin-edge.io
 Description: CLI tool use to control and configure thin-edge.io
  tedge provides:
  * mqtt publish/subscribe
  * configuration get/set
  * connect/disconnect cloud mappers
 Vcs-Browser: https://github.com/thin-edge/thin-edge.io
 Vcs-Git: https://github.com/thin-edge/thin-edge.io
"#;
        let meta_info = PackageMetadata {
            file_path: PathBuf::from("/"),
            metadata: contents.into(),
            remove_modified: false,
        };

        // Fail
        let res = meta_info.metadata_contains_all(&[&format!("Version: {}", &"1.5.1")]);
        assert!(
            res.is_err(),
            "expected error as there is a version mismatch"
        );
        let error_message = res.unwrap_err().to_string();
        assert!(
            error_message.contains("1:1.5.1"),
            "expected error message to contain the full package version"
        );

        // Pass
        let res = meta_info.metadata_contains_all(&[&format!("Version: {}", &"1:1.5.1")]);
        assert!(res.is_ok());
    }
}
