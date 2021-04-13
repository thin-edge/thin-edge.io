use crate::cli::config::ConfigKey;
use crate::command::{Command, ExecutionContext};
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

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        let mut config = self.config_repository.load()?;
        let () = (self.config_key.unset)(&mut config)?;
        self.config_repository.store(config)?;
        Ok(())
    }
}
