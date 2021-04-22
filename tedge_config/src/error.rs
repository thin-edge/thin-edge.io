#[derive(thiserror::Error, Debug)]
pub enum TEdgeConfigError {
    #[error("TOML parse error")]
    TOMLParseError(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    InvalidTOMLError(#[from] toml::ser::Error),

    #[error("I/O error")]
    IOError(#[from] std::io::Error),

    #[error(transparent)]
    ConfigSettingError(#[from] crate::ConfigSettingError),

    #[error(transparent)]
    InvalidConfigUrl(#[from] crate::models::InvalidConnectUrl),

    #[error("Config file not found: {0}")]
    ConfigFileNotFound(std::path::PathBuf),
}
