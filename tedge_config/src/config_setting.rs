pub trait ConfigSetting {
    /// This is something like `device.id`.
    const EXTERNAL_KEY: &'static str;

    const DESCRIPTION: &'static str;

    /// The underlying value type of the configuration setting.
    type Value;
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

    #[error("Infallible Error")]
    Infallible(#[from] std::convert::Infallible),
}
