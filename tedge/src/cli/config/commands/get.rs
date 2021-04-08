use crate::cli::config::ConfigKey;
use crate::command::{Command, ExecutionContext};
use crate::config::*;

pub struct GetConfigCommand {
    pub key: ConfigKey,
    pub config: TEdgeConfig,
}

impl Command for GetConfigCommand {
    fn description(&self) -> String {
        format!("get the configuration value for key: {}", self.key.as_str())
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        match self.config.get_config_value(self.key.as_str())? {
            None => println!(
                "The provided config key: '{}' is not set",
                self.key.as_str()
            ),
            Some(value) => println!("{}", value),
        }
        Ok(())
    }
}
