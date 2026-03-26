use camino::Utf8PathBuf;
use tedge_utils::fs::persist_file_with_template;

pub struct DeviceProfileManagerBuilder {}

impl DeviceProfileManagerBuilder {
    pub async fn try_new(ops_dir: &Utf8PathBuf) -> Result<Self, anyhow::Error> {
        let workflow_definition = include_str!("../resources/device_profile.toml");

        // Initialize device_profile.toml with template pattern:
        // - Always update device_profile.toml.template with the latest definition
        // - Only update device_profile.toml if it doesn't exist or hasn't been customized by the user
        persist_file_with_template(ops_dir, "device_profile.toml", workflow_definition).await?;

        Ok(Self {})
    }
}
