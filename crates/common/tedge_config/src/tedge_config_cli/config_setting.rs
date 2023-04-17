use super::new_tedge_config::WritableKey;

pub trait ConfigSetting {
    /// This is something like `device.id`.
    const KEY: &'static str;

    const DESCRIPTION: &'static str;

    /// The underlying value type of the configuration setting.
    type Value;
}

pub trait ConfigSettingAccessor<T: ConfigSetting> {
    /// Read a configuration setting
    fn query(&self, setting: T) -> ConfigSettingResult<T::Value>;

    fn query_optional(&self, setting: T) -> ConfigSettingResult<Option<T::Value>> {
        match self.query(setting) {
            Ok(value) => Ok(Some(value)),
            Err(ConfigSettingError::ConfigNotSet { .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// Extension trait that provides methods to query a setting as a String or
/// update a setting provided a String value.
pub trait ConfigSettingAccessorStringExt<T: ConfigSetting>: ConfigSettingAccessor<T> {
    /// Read a configuration setting and convert it into a String.
    fn query_string(&self, setting: T) -> ConfigSettingResult<String>;

    fn query_string_optional(&self, setting: T) -> ConfigSettingResult<Option<String>> {
        match self.query_string(setting) {
            Ok(value) => Ok(Some(value)),
            Err(ConfigSettingError::ConfigNotSet { .. }) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

pub type ConfigSettingResult<T> = Result<T, ConfigSettingError>;

#[derive(thiserror::Error, Debug)]
pub enum ConfigSettingError {
    #[error(
        r#"A value for '{key}' is missing.\n\
    A value can be set with `tedge config set {key} <value>`"#
    )]
    ConfigNotSet { key: &'static str },

    #[error("Cannot write to read-only setting: {message}")]
    WriteToReadOnlySetting { message: &'static str },

    #[error("Conversion from String failed")]
    ConversionFromStringFailed,

    #[error("Conversion into String failed")]
    ConversionIntoStringFailed,

    #[error("Derivation for '{key}' failed: {cause}")]
    DerivationFailed {
        key: &'static str,
        #[source]
        cause: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Config value '{key}', cannot be configured: {message} ")]
    SettingIsNotConfigurable {
        key: &'static str,
        message: &'static str,
    },

    #[error("An error occurred: {msg}")]
    Other { msg: &'static str },

    #[error("Unrecognised configuration key '{key}'")]
    WriteUnrecognisedKey {
        /// The key that was requested
        key: String,
    },

    #[error("Unrecognised configuration key: '{key}'")]
    ReadUnrecognisedKey {
        /// The key that was requested
        key: String,
    },

    #[error("Failed to deserialize '{key}': {error}")]
    Figment {
        /// The key that was requested
        key: WritableKey,

        #[source]
        /// The underlying error when deserializing that value
        error: figment::Error,
    },

    #[error("'{key}' could not be read:\n{message}")]
    ReadOnlySettingNotConfigured {
        key: &'static str,
        message: &'static str,
    },
}
