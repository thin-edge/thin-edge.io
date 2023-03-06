use crate::cli::config::ConfigKey;
use crate::command::Command;
use tedge_config::*;

pub struct UnsetConfigCommand {
    pub config_key: ConfigKey,
    pub config_repository: TEdgeConfigRepository,
}

impl Command for UnsetConfigCommand {
    fn description(&self) -> String {
        format!(
            "unset the configuration value for key: {}",
            self.config_key.key
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_repository.update_toml(&self.config_key.unset)?;
        Ok(())
    }
}
