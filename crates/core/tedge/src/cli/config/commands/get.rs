use tedge_config::tedge_toml::ReadableKey;
use tedge_config::TEdgeConfig;

use crate::command::Command;
use crate::log::MaybeFancy;

pub struct GetConfigCommand {
    pub key: ReadableKey,
}

#[async_trait::async_trait]
impl Command for GetConfigCommand {
    fn description(&self) -> String {
        format!("get the configuration value for key: '{}'", self.key)
    }

    async fn execute(&self, tedge_config: TEdgeConfig) -> Result<(), MaybeFancy<anyhow::Error>> {
        match tedge_config.read_string(&self.key) {
            Ok(value) => {
                println!("{}", value);
            }
            Err(tedge_config::tedge_toml::ReadError::ConfigNotSet { .. }) => {
                eprintln!("The provided config key: '{}' is not set", self.key);
                std::process::exit(1)
            }
            Err(tedge_config::tedge_toml::ReadError::ReadOnlyNotFound { message, key }) => {
                eprintln!("The provided config key: '{key}' is not configured: {message}",);
                std::process::exit(1)
            }
            Err(err) => return Err(anyhow::Error::new(err).into()),
        }

        Ok(())
    }
}
