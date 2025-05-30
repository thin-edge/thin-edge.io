use crate::command::Command;
use crate::log::MaybeFancy;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;

pub struct UnsetConfigCommand {
    pub key: WritableKey,
}

#[async_trait::async_trait]
impl Command for UnsetConfigCommand {
    fn description(&self) -> String {
        format!("unset the configuration value for '{}'", self.key)
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        tedge_config
            .update_toml(&|dto, _reader| Ok(dto.try_unset_key(&self.key)?))
            .await
            .map_err(anyhow::Error::new)?;
        tracing::info!(target: "Audit", "tedge config unset {}", &self.key);
        Ok(())
    }
}
