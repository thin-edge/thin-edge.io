//! Configuration settings, as bootstrap reads and writes them

/// A `key=value` configuration update.
///
/// The key is validated at execution time,
/// against tedge config keys for built-in clouds
/// and against the mapper config for custom cloud mappers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
}

impl KeyValue {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    /// Parse a `KEY=VALUE` argument (the `--set` value parser)
    pub fn parse(input: &str) -> Result<Self, String> {
        match input.split_once('=') {
            Some((key, value)) if !key.is_empty() => Ok(Self::new(key, value)),
            _ => Err(format!("expected KEY=VALUE, got {input:?}")),
        }
    }
}

/// The instance-scoped settings bootstrap reads and writes,
/// relative to the instance
/// (`c8y.`, `c8y.profiles.<p>.` in the tedge config,
/// or the top level of a custom mapper's `mapper.toml`)
pub mod key {
    pub const URL: &str = "url";
    pub const HTTP: &str = "http";
    pub const MQTT: &str = "mqtt";
    pub const AUTH_METHOD: &str = "auth_method";
    pub const CREDENTIALS_PATH: &str = "credentials_path";
    pub const ROOT_CERT_PATH: &str = "root_cert_path";
    /// The custom mapper's trust store (`device.root_cert_path` in its mapper.toml)
    pub const DEVICE_ROOT_CERT_PATH: &str = "device.root_cert_path";
    pub const CLOUD_TYPE: &str = "cloud_type";
    pub const BRIDGE_TOPIC_PREFIX: &str = "bridge.topic_prefix";
    pub const PROXY_BIND_PORT: &str = "proxy.bind.port";
    /// For built-in clouds, this is the device-global identity, not an instance key
    pub const DEVICE_ID: &str = "device.id";
    pub const DEVICE_CERT_PATH: &str = "device.cert_path";
    pub const DEVICE_KEY_PATH: &str = "device.key_path";
    pub const DEVICE_CSR_PATH: &str = "device.csr_path";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_pairs_are_parsed_and_validated() {
        assert_eq!(
            KeyValue::parse("c8y.url=example.com").unwrap(),
            KeyValue::new("c8y.url", "example.com")
        );
        // an empty value is allowed (it unsets nothing, but is a valid pair)
        assert_eq!(KeyValue::parse("c8y.url=").unwrap().value, "");
        assert!(KeyValue::parse("=value").is_err());
        assert!(KeyValue::parse("novalue").is_err());
    }
}
