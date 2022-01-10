use crate::size_threshold::SizeThresholdExceeded;
use mqtt_client::MqttClientError;
use tedge_config::TEdgeConfigError;
use thin_edge_json::serialize::ThinEdgeJsonSerializationError;

#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error(transparent)]
    FromMqttClient(#[from] MqttClientError),

    #[error("Home directory is not found.")]
    HomeDirNotFound,

    #[error(transparent)]
    FromTEdgeConfig(#[from] TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromFlockfile(#[from] flockfile::FlockfileError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error(transparent)]
    FromMapper(#[from] MapperError),

    #[error(transparent)]
    FromCumulocityJsonError(#[from] c8y_translator::json::CumulocityJsonError),

    #[error(transparent)]
    FromThinEdgeJsonSerialization(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    FromThinEdgeJsonDeserialization(#[from] thin_edge_json::alarm::ThinEdgeJsonDeserializerError),

    #[error(transparent)]
    FromThinEdgeJsonParser(#[from] thin_edge_json::parser::ThinEdgeJsonParserError),

    #[error(transparent)]
    FromSizeThresholdExceeded(#[from] SizeThresholdExceeded),

    #[error("The given Child ID '{id}' is invalid.")]
    InvalidChildId { id: String },

    #[error(transparent)]
    FromMqttClient(#[from] MqttClientError),

    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromSmartRestSerializerError(#[from] c8y_smartrest::error::SmartRestSerializerError),

    #[error("Unsupported topic: {0}")]
    UnsupportedTopic(String),
}

#[derive(Debug, thiserror::Error)]
pub enum OperationsError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),
}
