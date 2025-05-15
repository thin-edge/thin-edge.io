use crate::command::Command;
use crate::log::MaybeFancy;
use tedge_config::tedge_toml::WritableKey;
use tedge_config::TEdgeConfig;

pub struct SetConfigCommand {
    pub key: WritableKey,
    pub value: String,
    pub config: TEdgeConfig,
}

#[async_trait::async_trait]
impl Command for SetConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: '{}' with value: {}.",
            self.key.to_cow_str(),
            self.value
        )
    }

    async fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        self.config
            .location()
            .update_toml(&|dto, _reader| {
                dto.try_update_str(&self.key, &self.value)
                    .map_err(|e| e.into())
            })
            .await
            .map_err(anyhow::Error::new)?;
        Ok(())
    }
}
