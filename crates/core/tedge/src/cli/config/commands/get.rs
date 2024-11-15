use tedge_config::ReadableKey;

use crate::command::Command;
use crate::log::MaybeFancy;

pub struct GetConfigCommand {
    pub key: ReadableKey,
    pub config: tedge_config::TEdgeConfig,
}

impl Command for GetConfigCommand {
    fn description(&self) -> String {
        format!("get the configuration value for key: '{}'", self.key)
    }

    fn execute(&self) -> Result<(), MaybeFancy<anyhow::Error>> {
        match self.config.read_string(&self.key) {
            Ok(value) => {
                println!("{}", value);
            }
            Err(tedge_config::ReadError::ConfigNotSet { .. }) => {
                eprintln!("The provided config key: '{}' is not set", self.key);
                std::process::exit(1)
            }
            Err(tedge_config::ReadError::ReadOnlyNotFound { message, key }) => {
                eprintln!("The provided config key: '{key}' is not configured: {message}",);
                std::process::exit(1)
            }
            Err(err) => return Err(anyhow::Error::new(err).into()),
        }

        Ok(())
    }
}
