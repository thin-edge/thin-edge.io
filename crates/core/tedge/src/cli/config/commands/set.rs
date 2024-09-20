use crate::command::Command;
use tedge_config::TEdgeConfigLocation;
use tedge_config::WritableKey;

pub struct SetConfigCommand {
    pub key: WritableKey,
    pub value: String,
    pub config_location: TEdgeConfigLocation,
}

impl Command for SetConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: '{}' with value: {}.",
            self.key.as_str(),
            self.value
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_location.update_toml(&|dto, _reader| {
            dto.try_update_str(&self.key, &self.value)
                .map_err(|e| e.into())
        })?;
        Ok(())
    }
}
