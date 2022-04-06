use thiserror::Error;

/// An error that a plugin might emit
#[derive(Error, Debug)]
pub enum PluginError {
    /// Error kind if the configuration of the plugin was faulty
    #[error("An error in the configuration was found")]
    Configuration(#[from] toml::de::Error),

    /// Error kind to report any `anyhow::Error` error
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
}
