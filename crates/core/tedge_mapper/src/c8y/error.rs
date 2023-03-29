use c8y_api::smartrest::error::OperationsError;
use c8y_api::smartrest::error::SMCumulocityMapperError;
use c8y_api::smartrest::error::SmartRestDeserializerError;
use c8y_api::smartrest::error::SmartRestSerializerError;
use plugin_sm::operation_logs::OperationLogsError;

#[derive(thiserror::Error, Debug)]
#[allow(clippy::large_enum_variant)]
#[allow(clippy::enum_variant_names)]
pub enum CumulocityMapperError {
    #[error(transparent)]
    InvalidTopicError(#[from] tedge_api::TopicError),

    #[error(transparent)]
    InvalidThinEdgeJson(#[from] tedge_api::SoftwareError),

    #[error(transparent)]
    FromElapsed(#[from] tokio::time::error::Elapsed),

    #[error(transparent)]
    FromMqttClient(#[from] mqtt_channel::MqttError),

    #[error(transparent)]
    FromReqwest(#[from] reqwest::Error),

    #[error(transparent)]
    FromSmartRestSerializer(#[from] SmartRestSerializerError),

    #[error(transparent)]
    FromSmartRestDeserializer(#[from] SmartRestDeserializerError),

    #[error(transparent)]
    FromSmCumulocityMapperError(#[from] SMCumulocityMapperError),

    #[error(transparent)]
    FromTedgeConfig(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromTimeFormat(#[from] time::error::Format),

    #[error(transparent)]
    FromTimeParse(#[from] time::error::Parse),

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromSerde(#[from] serde_json::Error),

    #[error("Operation execution failed: {error_message}. Command: {command}. Operation name: {operation_name}")]
    ExecuteFailed {
        error_message: String,
        command: String,
        operation_name: String,
    },

    #[error("Failed to read the child device operations in directory: {dir}")]
    ReadDirError { dir: std::path::PathBuf },

    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromOperationLogs(#[from] OperationLogsError),

    #[error(transparent)]
    TedgeConfig(#[from] tedge_config::TEdgeConfigError),
}
