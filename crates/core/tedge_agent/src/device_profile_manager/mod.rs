use camino::Utf8PathBuf;
use tedge_utils::file::create_file_with_defaults;
use tedge_utils::file::FileError;

pub struct DeviceProfileManagerBuilder {}

impl DeviceProfileManagerBuilder {
    pub async fn try_new(ops_dir: &Utf8PathBuf) -> Result<Self, FileError> {
        let workflow_file = ops_dir.join("device_profile.toml");
        if !workflow_file.exists() {
            let workflow_definition = include_str!("../resources/device_profile.toml");

            create_file_with_defaults(workflow_file, Some(workflow_definition)).await?;
        }
        Ok(Self {})
    }
}
