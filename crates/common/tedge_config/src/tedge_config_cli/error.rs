#[derive(thiserror::Error, Debug)]
pub enum TEdgeConfigError {
    #[error("TOML parse error")]
    FromTOMLParse(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    FromInvalidTOML(#[from] toml::ser::Error),

    #[error("I/O error")]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromConfigSetting(#[from] crate::ConfigSettingError),

    #[error(transparent)]
    FromInvalidConfigUrl(#[from] crate::tedge_config_cli::models::InvalidConnectUrl),

    #[error("Config file not found: {0}")]
    ConfigFileNotFound(std::path::PathBuf),

    #[error("Home directory is not found.")]
    HomeDirNotFound,
}
