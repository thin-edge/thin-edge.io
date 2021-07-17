use crate::cli::config::ConfigKey;
use crate::command::Command;

pub struct GetConfigCommand {
    pub config_key: ConfigKey,
    pub config: tedge_config::TEdgeConfig,
}

impl Command for GetConfigCommand {
    fn description(&self) -> String {
        format!(
            "get the configuration value for key: {}",
            self.config_key.key
        )
    }

    fn execute(&self) -> anyhow::Result<()> {
        match (self.config_key.get)(&self.config) {
            Ok(value) => {
                println!("{}", value);
            }
            Err(tedge_config::ConfigSettingError::ConfigNotSet { .. }) => {
                println!(
                    "The provided config key: '{}' is not set",
                    self.config_key.key
                );
            }
            Err(err) => return Err(err.into()),
        }

        Ok(())
    }
}
