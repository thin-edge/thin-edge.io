use crate::cli::config::ConfigKey;
use crate::command::{Command, ExecutionContext};
use tedge_config::*;

pub struct SetConfigCommand {
    pub config_key: ConfigKey,
    pub value: String,
    pub config: TEdgeConfig,
    pub config_repository: TEdgeConfigRepository,
}

impl Command for SetConfigCommand {
    fn description(&self) -> String {
        format!(
            "set the configuration key: {} with value: {}.",
            self.config_key.key, self.value
        )
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let mut config = self.config.clone();
        let () = (self.config_key.set)(&mut config, self.value.to_string())?;
        self.config_repository.store(config)?;
        Ok(())
    }
}
