use crate::command::Command;
use tedge_config::TEdgeConfigLocation;
use tedge_config::WritableKey;

pub struct UnsetConfigCommand {
    pub key: WritableKey,
    pub config_location: TEdgeConfigLocation,
}

impl Command for UnsetConfigCommand {
    fn description(&self) -> String {
        format!("unset the configuration value for '{}'", self.key)
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_location
            .update_toml(&|dto, _reader| Ok(dto.try_unset_key(&self.key)?))?;
        Ok(())
    }
}
