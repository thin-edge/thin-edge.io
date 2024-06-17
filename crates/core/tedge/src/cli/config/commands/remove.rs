use crate::command::Command;
use tedge_config::TEdgeConfigLocation;
use tedge_config::WritableKey;

pub struct RemoveConfigCommand {
    pub key: WritableKey,
    pub value: String,
    pub config_location: TEdgeConfigLocation,
}

impl Command for RemoveConfigCommand {
    fn description(&self) -> String {
        format!("Remove or unset the configuration value for '{}'", self.key)
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_location.update_toml(&|dto, reader| {
            dto.try_remove_str(reader, self.key, &self.value)
                .map_err(|e| e.into())
        })?;
        Ok(())
    }
}
