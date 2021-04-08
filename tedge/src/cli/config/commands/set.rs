use crate::cli::config::WritableConfigKey;
use crate::command::{Command, ExecutionContext};
use crate::config::*;

pub struct SetConfigCommand {
    pub key: WritableConfigKey,
    pub value: String,
    pub config: TEdgeConfig,
}

impl Command for SetConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: {} with value: {}.",
            self.key.as_str(),
            self.value
        )
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let mut config = self.config.clone();
        config.set_config_value(self.key.as_str(), self.value.to_string())?;
        config.write_to_default_config()?;
        Ok(())
    }
}
