use camino::Utf8PathBuf;
use tedge_utils::paths::TedgePaths;

pub struct DeviceProfileManagerBuilder {}

impl DeviceProfileManagerBuilder {
    pub async fn try_new(ops_dir: &Utf8PathBuf) -> Result<Self, anyhow::Error> {
        let workflow_definition = include_str!("../resources/device_profile.toml");

        // Initialize device_profile.toml with template pattern:
        // - Always update device_profile.toml.template with the latest definition
        // - Only update device_profile.toml if it doesn't exist or hasn't been customized by the user
        TedgePaths::from_root_with_defaults(ops_dir.as_std_path(), "", "")
            .template_file("device_profile.toml")?
            .persist(workflow_definition)
            .await?;

        Ok(Self {})
    }
}
