use crate::config::{ConfigError, TEdgeConfig};
pub fn get_config_value(config: &TEdgeConfig, key: &str) -> Result<String, ConfigError> {
    config
        .get_config_value(key)?
        .ok_or_else(|| ConfigError::ConfigNotSet { key: key.into() })
}
