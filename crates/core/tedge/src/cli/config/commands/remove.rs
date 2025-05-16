use crate::command::Command;
use crate::log::MaybeFancy;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;

pub struct RemoveConfigCommand {
    pub key: WritableKey,
    pub value: String,
}

#[async_trait::async_trait]
impl Command for RemoveConfigCommand {
    fn description(&self) -> String {
        format!("Remove or unset the configuration value for '{}'", self.key)
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        tedge_config
            .update_toml(&|dto, reader| {
                dto.try_remove_str(reader, &self.key, &self.value)
                    .map_err(|e| e.into())
            })
            .await
            .map_err(anyhow::Error::new)?;
        Ok(())
    }
}
