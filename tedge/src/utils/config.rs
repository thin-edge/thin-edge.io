use crate::config::{ConfigError, TEdgeConfig};

pub fn get_config_value(config: &TEdgeConfig, key: &str) -> Result<String, ConfigError> {
    config
        .get_config_value(key)?
        .ok_or_else(|| ConfigError::ConfigNotSet { key: key.into() })
}

pub fn get_config_value_or_default(
    config: &TEdgeConfig,
    key: &str,
    default: &str,
) -> Result<String, ConfigError> {
    let value = config
        .get_config_value(key)?
        .unwrap_or_else(|| default.into());

    Ok(value)
}

pub fn update_config_with_value(
    config: &mut TEdgeConfig,
    key: &str,
    value: &str,
) -> Result<(), ConfigError> {
    let _ = config.set_config_value(key, value.to_owned())?;
    Ok(config.write_to_default_config()?)
}

pub fn parse_user_provided_address(input: String) -> Result<String, ConfigError> {
    if input.contains(':') {
        return Err(ConfigError::InvalidConfigUrl(input));
    }

    Ok(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_provided_address_should_return_err_provided_address_with_port() {
        let input = "test.address.com:8883";

        assert!(parse_user_provided_address(input.into()).is_err());
    }

    #[test]
    fn parse_user_provided_address_should_return_err_provided_address_with_scheme_http() {
        let input = "http://test.address.com";

        assert!(parse_user_provided_address(input.into()).is_err());
    }

    #[test]
    fn parse_user_provided_address_should_return_err_provided_address_with_port_and_http() {
        let input = "http://test.address.com:8883";

        assert!(parse_user_provided_address(input.into()).is_err());
    }

    #[test]
    fn parse_user_provided_address_should_return_string() {
        let input = "test.address.com";
        let expected = "test.address.com".to_owned();

        assert_eq!(parse_user_provided_address(input.into()).unwrap(), expected);
    }
}
