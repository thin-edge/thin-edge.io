use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("An error in the configuration was found")]
    Configuration(#[from] toml::de::Error),
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
}
