use c8y_smartrest::error::{
    SMCumulocityMapperError, SmartRestDeserializerError, SmartRestSerializerError,
};
use plugin_sm::operation_logs::OperationLogsError;

#[derive(thiserror::Error, Debug)]
#[allow(clippy::large_enum_variant)]
#[allow(clippy::enum_variant_names)]
pub enum CumulocityMapperError {
    #[error(transparent)]
    InvalidTopicError(#[from] agent_interface::TopicError),

    #[error(transparent)]
    InvalidThinEdgeJson(#[from] agent_interface::SoftwareError),

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

    #[error("Operation execution failed: {error_message}. Command: {command}. Operation name: {operation_name}")]
    ExecuteFailed {
        error_message: String,
        command: String,
        operation_name: String,
    },

    #[error(transparent)]
    FromOperationLogs(#[from] OperationLogsError),

    #[error(transparent)]
    TedgeConfig(#[from] tedge_config::TEdgeConfigError),
}
