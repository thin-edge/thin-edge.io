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

    #[error("Error from directory")]
    DirectoryError(#[from] DirectoryError),
}

#[derive(Error, Debug)]
pub enum DirectoryError {
    #[error("Plugin named '{}' not found", .0)]
    PluginNameNotFound(String),

    #[error("Plugin '{}' does not support the following message types: {}", .0 ,.1.join(","))]
    PluginDoesNotSupport(String, Vec<&'static str>),
}
