use serde::Deserialize;
use std::fs;
use std::path::Path;
use tracing::error;
use tracing::info;

#[derive(Debug, Eq, PartialEq, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirmwareEntry {
    name: String,
    version: String,
    url: String,
    sha256: Option<String>,
}

impl FirmwareEntry {
    fn new(config_file_path: &Path) -> Self {
        let path_str = config_file_path.display().to_string();
        info!("Reading the firmware info from {}", path_str);
        match fs::read_to_string(config_file_path) {
            Ok(contents) => match toml::from_str(contents.as_str()) {
                Ok(config) => config,
                Err(err) => {
                    error!("The config file {path_str} is malformed. {err}");
                    Self::default()
                }
            },
            Err(err) => {
                error!("The config file {path_str} does not exist or is not readable. {err}");
                Self::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tedge_test_utils::fs::TempTedgeDir;
    use test_case::test_case;

    #[test]
    fn deserialize_firmware_entry() {
        let entry: FirmwareEntry = toml::from_str(
            r#"
            name = "my firmware"
            version = "1.0"
            url = "http://example.com/
            "#,
        )
        .unwrap();
        assert_eq!(
            entry,
            FirmwareEntry {
                name: "my firmware".to_string(),
                version: "1.0".to_string(),
                url: "http://example.com/".to_string(),
                sha256: None,
            }
        );
    }
}
