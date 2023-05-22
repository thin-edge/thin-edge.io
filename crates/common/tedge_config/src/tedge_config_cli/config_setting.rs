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

    /// Update a configuration setting
    fn update(&mut self, _setting: T, _value: T::Value) -> ConfigSettingResult<()>;

    /// Unset a configuration setting / reset to default
    fn unset(&mut self, _setting: T) -> ConfigSettingResult<()>;
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

    /// Update a configuration setting from a String value
    fn update_string(&mut self, setting: T, value: String) -> ConfigSettingResult<()>;
}

pub type ConfigSettingResult<T> = Result<T, ConfigSettingError>;

#[derive(thiserror::Error, Debug)]
pub enum ConfigSettingError {
    #[error(
        r#"A value for `{key}` is missing.
    A value can be set with `tedge config set {key} <value>`"#
    )]
    ConfigNotSet { key: &'static str },

    #[error("Readonly setting: {message}")]
    ReadonlySetting { message: &'static str },

    #[error("Conversion from String failed")]
    ConversionFromStringFailed,

    #[error("Conversion into String failed")]
    ConversionIntoStringFailed,

    #[error("Derivation for `{key}` failed: {cause}")]
    DerivationFailed { key: &'static str, cause: String },

    #[error("Config value {key}, cannot be configured: {message} ")]
    SettingIsNotConfigurable {
        key: &'static str,
        message: &'static str,
    },

    #[error("An error occurred: {msg}")]
    Other { msg: &'static str },

    #[error(transparent)]
    Write(#[from] crate::new::WriteError),
}
