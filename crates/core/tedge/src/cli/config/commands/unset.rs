use crate::command::Command;
use tedge_config::*;

pub struct UnsetConfigCommand {
    pub config_key: WritableKey,
    pub config_repository: TEdgeConfigRepository,
}

impl Command for UnsetConfigCommand {
    fn description(&self) -> String {
        format!(
            "unset the configuration value for key: '{}'",
            self.config_key
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_repository.unset(self.config_key)?;
        Ok(())
    }
}
