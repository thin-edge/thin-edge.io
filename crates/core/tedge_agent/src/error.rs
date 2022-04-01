use std::path::PathBuf;

use crate::journal::JournalError;
use agent_interface::{ApiError, SoftwareError};
use flockfile::FlockfileError;
use mqtt_channel::MqttError;
use tedge_config::{ConfigSettingError, TEdgeConfigError};

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
    NoPlugins { plugins_path: PathBuf },

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    FromSoftware(#[from] SoftwareError),

    #[error(transparent)]
    FromAPI(#[from] ApiError),

    #[error(transparent)]
    FromState(#[from] StateError),

    #[error(transparent)]
    FromJournal(#[from] JournalError),

    #[error(transparent)]
    FromTedgeConfig(#[from] TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] ConfigSettingError),

    #[error(transparent)]
    FromFlockfileError(#[from] FlockfileError),

    #[error("Command returned non 0 exit code.")]
    CommandFailed,

    #[error("Failed parsing /proc/uptime")]
    UptimeParserError,

    #[error("Failed to cast string to float.")]
    FloatCastingError,

    #[error("Could not convert {timestamp:?} to unix timestamp. Error message: {}")]
    TimestampConversionError { timestamp: i64, error_msg: String },

    #[error(transparent)]
    FromOperationsLogs(#[from] crate::operation_logs::OperationLogsError),
}

#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum StateError {
    #[error(transparent)]
    FromTOMLParse(#[from] toml::de::Error),

    #[error(transparent)]
    FromInvalidTOML(#[from] toml::ser::Error),

    #[error(transparent)]
    FromIo(#[from] std::io::Error),
}
