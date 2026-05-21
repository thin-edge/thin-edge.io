use tedge_utils::paths::ManagedDir;

pub struct DeviceProfileManagerBuilder {}

impl DeviceProfileManagerBuilder {
    pub async fn try_new(ops_dir: &ManagedDir) -> Result<Self, anyhow::Error> {
        let workflow_definition = include_str!("../resources/device_profile.toml");

        // Initialize device_profile.toml with template pattern:
        // - Always update device_profile.toml.template with the latest definition
        // - Only update device_profile.toml if it doesn't exist or hasn't been customized by the user
        ops_dir
            .template_file("device_profile.toml")?
            .persist(workflow_definition)
            .await?;

        Ok(Self {})
    }
}
