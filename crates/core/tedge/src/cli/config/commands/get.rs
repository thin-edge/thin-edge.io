use tedge_config::tedge_config_cli::new_tedge_config::ReadableKey;
use tedge_config::ConfigSettingError;

use crate::command::Command;

pub struct GetConfigCommand {
    pub config_key: ReadableKey,
    pub config: tedge_config::tedge_config_cli::new_tedge_config::NewTEdgeConfig,
}

impl Command for GetConfigCommand {
    fn description(&self) -> String {
        format!("get the configuration value for key: '{}'", self.config_key)
    }

    fn execute(&self) -> anyhow::Result<()> {
        match self.config.read(self.config_key) {
            Ok(Some(value)) => {
                println!("{}", value);
            }
            Ok(None) => {
                eprintln!(
                    "The provided config key: '{}' is not set",
                    self.config_key.as_str()
                );
            }
            Err(e @ ConfigSettingError::ReadOnlySettingNotConfigured { .. }) => {
                eprintln!(
                    "The provided config key: '{}' is not set. {e}",
                    self.config_key.as_str()
                );
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }
}
