use crate::command::Command;
use tedge_config::*;

pub struct SetConfigCommand {
    pub config_key: WritableKey,
    pub value: String,
    pub config_repository: TEdgeConfigRepository,
}

impl Command for SetConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: '{}' with value: {}.",
            self.config_key, self.value
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        self.config_repository
            .update_string(self.config_key, &self.value)?;
        Ok(())
    }
}
