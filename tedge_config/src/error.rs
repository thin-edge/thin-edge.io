#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("TOML parse error")]
    TOMLParseError(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    InvalidTOMLError(#[from] toml::ser::Error),

    #[error("I/O error")]
    IOError(#[from] std::io::Error),

    #[error("Home directory not found")]
    HomeDirectoryNotFound,

    #[error("Invalid characters found in home directory path")]
    InvalidCharacterInHomeDirectoryPath,

    #[error(transparent)]
    ConfigSettingError(#[from] crate::ConfigSettingError),

    #[error("Config file not found: {0}")]
    ConfigFileNotFound(std::path::PathBuf),
}
