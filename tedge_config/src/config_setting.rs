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

pub type ConfigSettingResult<T> = Result<T, ConfigSettingError>;

#[derive(thiserror::Error, Debug)]
pub enum ConfigSettingError {
    #[error(
        r#"A value for `{key}` is missing.
    A value can be set with `tedge config set {key} <value>`"#
    )]
    ConfigNotSet { key: &'static str },

    // XXX
    #[error(
        r#"Provided URL: '{0}' contains scheme or port.
    Provided URL should contain only domain, eg: 'subdomain.cumulocity.com'."#
    )]
    InvalidConfigUrl(String),

    #[error("Readonly setting")]
    ReadonlySetting,

    #[error("Infallible Error")]
    Infallible(#[from] std::convert::Infallible),
}
