use crate::cli::config::WritableConfigKey;
use crate::command::{Command, ExecutionContext};
use crate::config::*;

pub struct UnsetConfigCommand {
    pub key: WritableConfigKey,
    pub config: TEdgeConfig,
}

impl Command for UnsetConfigCommand {
    fn description(&self) -> String {
        format!(
            "unset the configuration value for key: {}",
            self.key.as_str()
        )
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let mut config = self.config.clone();
        config.unset_config_value(self.key.as_str())?;
        config.write_to_default_config()?;
        Ok(())
    }
}
