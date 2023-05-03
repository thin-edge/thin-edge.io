use crate::http_server::error::FileTransferError;
use camino::Utf8PathBuf;
use flockfile::FlockfileError;
use mqtt_channel::MqttError;
use tedge_actors::RuntimeError;
use tedge_api::SoftwareError;
use tedge_config::ConfigSettingError;
use tedge_config::TEdgeConfigError;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum AgentError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("An internal task failed to complete.")]
    FromJoin(#[from] tokio::task::JoinError),

    #[error(transparent)]
    FromMqttClient(#[from] MqttError),

    #[error("Couldn't load plugins from {plugins_path}")]
    NoPlugins { plugins_path: Utf8PathBuf },

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    FromSoftware(#[from] SoftwareError),

    #[error(transparent)]
    FromState(#[from] crate::state_repository::error::StateError),

    #[error(transparent)]
    FromTedgeConfig(#[from] TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] ConfigSettingError),

    #[error(transparent)]
    FromSystemServices(#[from] tedge_config::system_services::SystemServiceError),

    #[error(transparent)]
    FromFlockfileError(#[from] FlockfileError),

    #[error("Command returned non 0 exit code.")]
    CommandFailed,

    #[error("Failed parsing /proc/uptime")]
    UptimeParserError,

    #[error("Failed to cast string to float.")]
    FloatCastingError,

    #[error("Could not convert {timestamp:?} to unix timestamp. Error message: {error_msg}")]
    TimestampConversionError { timestamp: i64, error_msg: String },

    #[error(transparent)]
    FromOperationsLogs(#[from] plugin_sm::operation_logs::OperationLogsError),

    #[error(transparent)]
    FromFileTransferError(#[from] FileTransferError),

    #[error(transparent)]
    FromRestartManagerError(#[from] crate::restart_manager::error::RestartManagerError),
}

impl From<AgentError> for RuntimeError {
    fn from(error: AgentError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}
