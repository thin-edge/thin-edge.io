use crate::command::Command;
use crate::config::restrict_cloud_config_update;
use crate::log::MaybeFancy;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;

pub struct AddConfigCommand {
    pub key: WritableKey,
    pub value: String,
}

#[async_trait::async_trait]
impl Command for AddConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: '{}' with value: {}.",
            self.key.to_cow_str(),
            self.value
        )
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        restrict_cloud_config_update("add", &self.key, &tedge_config).await?;
        tedge_config
            .update_toml(&|dto, reader| {
                dto.try_append_str(reader, &self.key, &self.value)
                    .map_err(|e| e.into())
            })
            .await
            .map_err(anyhow::Error::new)?;
        Ok(())
    }
}
