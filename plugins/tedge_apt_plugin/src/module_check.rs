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
            if !self.metadata.contains(pattern) {
                // Each pattern is a "Key: expected_value" entry. The value may itself
                // contain a colon (e.g. an epoch prefix "Version: 1:2.3.4-1"), so split
                // only on the first colon. A pattern without a colon is treated as a key
                // with no requested value.
                let (expected_key, provided_value) = match pattern.split_once(':') {
                    Some((key, value)) => (key.trim(), value.trim()),
                    None => (pattern.trim(), ""),
                };

                // Recover the value the package actually declares for this key, looking it
                // up line by line. The metadata may not contain the field at all (e.g. a
                // malformed package missing the Package field), in which case the actual
                // value is left empty rather than panicking.
                let actual_value = self
                    .metadata
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(key, value)| {
                            (key.trim() == expected_key).then(|| value.trim().to_string())
                        })
                    })
                    .unwrap_or_default();

                return Err(InternalError::MetaDataMismatch {
                    package: self.file_path().to_string_lossy().to_string(),
                    expected_key: expected_key.to_string(),
                    expected_value: actual_value,
                    provided_value: provided_value.to_string(),
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
        let res = meta_info.metadata_contains_all(&[&format!("Version: {}", "1.5.1")]);
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
        let res = meta_info.metadata_contains_all(&[&format!("Version: {}", "1:1.5.1")]);
        assert!(res.is_ok());
    }

    #[test]
    fn validation_fails_cleanly_when_package_field_is_missing() {
        // A malformed package whose metadata does not carry a `Package` field,
        // e.g. an archive `dpkg -I` can still read but that is missing required fields
        let contents = r#"
new Debian package, version 2.0.
 size 438 bytes: control archive=199 bytes.
      92 bytes,     4 lines      control
 Version: 1.0.0
 Architecture: all
 Maintainer: thin-edge.io team <info@thin-edge.io>
 Description: a malformed archive without a name field
"#;
        let meta_info = PackageMetadata {
            file_path: PathBuf::from("/tmp/malformed.deb"),
            metadata: contents.into(),
            remove_modified: false,
        };

        let res = meta_info.metadata_contains_all(&[&format!("Package: {}", "someapp")]);
        assert!(
            res.is_err(),
            "expected a clean error rather than a panic when the field is missing"
        );
        let error_message = res.unwrap_err().to_string();
        assert!(
            error_message.contains("Package"),
            "expected error message to name the missing field, got: {error_message}"
        );
        assert!(
            error_message.contains("someapp"),
            "expected error message to mention the requested package, got: {error_message}"
        );
    }

    #[test]
    fn error_message_surfaces_actual_value_when_package_name_mismatches() {
        let contents = r#"
new Debian package, version 2.0.
 size 714 bytes: control archive=362 bytes.
      54 bytes,     1 lines      md5sums
 Package: sampledeb
 Version: 1.0.0
 Architecture: all
 Maintainer: thin-edge.io team <info@thin-edge.io>
 Description: My Sample App Debian Package
"#;
        let meta_info = PackageMetadata {
            file_path: PathBuf::from("/tmp/sampledeb.deb"),
            metadata: contents.into(),
            remove_modified: false,
        };

        let res = meta_info.metadata_contains_all(&[&format!("Package: {}", "wrongname")]);
        assert!(
            res.is_err(),
            "expected an error as the package name differs"
        );
        let error_message = res.unwrap_err().to_string();
        assert!(
            error_message.contains("sampledeb"),
            "expected error message to surface the package's actual name, got: {error_message}"
        );
        assert!(
            error_message.contains("wrongname"),
            "expected error message to mention the requested name, got: {error_message}"
        );
    }
}
