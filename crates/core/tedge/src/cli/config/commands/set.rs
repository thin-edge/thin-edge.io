use crate::cli::config::ConfigKey;
use crate::command::Command;
use tedge_config::*;

pub struct SetConfigCommand {
    pub config_key: ConfigKey,
    pub value: String,
    pub config_repository: TEdgeConfigRepository,
}

impl Command for SetConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: {} with value: {}.",
            self.config_key.key, self.value
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_repository
            .update_toml(&|config| (self.config_key.set)(config, self.value.to_string()))?;
        Ok(())
    }
}
