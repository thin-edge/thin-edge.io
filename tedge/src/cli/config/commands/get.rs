use crate::cli::config::config_keys::*;
use crate::command::{Command, ExecutionContext};
use tedge_config::*;

pub struct GetConfigCommand {
    pub key: ReadOnlyConfigKey,
    pub config: TEdgeConfig,
}

impl Command for GetConfigCommand {
    fn description(&self) -> String {
        format!("get the configuration value for key: {}", self.key.as_str())
    }

    fn execute(&self, _context: &ExecutionContext) -> Result<(), anyhow::Error> {
        match self.key.get_config_value(&self.config) {
            Ok(value) => println!("{}", value),
            Err(ConfigSettingError::ConfigNotSet { key }) => {
                println!("The provided config key: '{}' is not set", key)
            }
            Err(err) => return Err(err.into()),
        }
        Ok(())
    }
}
