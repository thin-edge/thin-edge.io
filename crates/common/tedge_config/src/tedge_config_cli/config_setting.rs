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
    Write(#[from] crate::WriteError),
}
