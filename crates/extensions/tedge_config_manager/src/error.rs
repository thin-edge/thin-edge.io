use tedge_actors::RuntimeError;

#[derive(thiserror::Error, Debug)]
pub enum ConfigManagementError {
    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),

    #[error("Failed to parse response with: {0}")]
    FromSerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    FromChannelError(#[from] tedge_actors::ChannelError),

    #[error(transparent)]
    InvalidConfigTypeError(#[from] InvalidConfigTypeError),

    #[error(transparent)]
    FromPathsError(#[from] tedge_utils::paths::PathsError),

    #[error(transparent)]
    FromIoError(#[from] std::io::Error),

    #[error(transparent)]
    FromFileError(#[from] tedge_utils::file::FileError),

    #[error("Received unexpected message on topic")]
    InvalidTopicError,

    #[error("Directory {path} is not found.")]
    DirectoryNotFound { path: std::path::PathBuf },

    #[error(transparent)]
    FromEntityTopicError(#[from] tedge_api::mqtt_topics::EntityTopicError),

    #[error(transparent)]
    FromAtomFileError(#[from] tedge_utils::fs::AtomFileError),

    #[error("Config plugin '{plugin_name}' error: {reason}")]
    PluginError { plugin_name: String, reason: String },

    #[error("{0:#}")]
    Other(#[from] anyhow::Error),
}

impl From<ConfigManagementError> for RuntimeError {
    fn from(error: ConfigManagementError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}

#[derive(thiserror::Error, Debug)]
#[error(
    "The requested config_type {config_type:?} is not defined in the plugin configuration file."
)]
pub struct InvalidConfigTypeError {
    pub config_type: String,
}
