use crate::config::ConfigError;

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
