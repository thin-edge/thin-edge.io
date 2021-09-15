use crate::size_threshold::SizeThresholdExceeded;
use mqtt_client::MqttClientError;
use tedge_config::TEdgeConfigError;
use thin_edge_json::serialize::ThinEdgeJsonSerializationError;

#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error(transparent)]
    MqttClientError(#[from] MqttClientError),

    #[error("Home directory is not found.")]
    HomeDirNotFound,

    #[error(transparent)]
    TEdgeConfigError(#[from] TEdgeConfigError),

    #[error(transparent)]
    ConfigSettingError(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FlockfileError(#[from] flockfile::FlockfileError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error(transparent)]
    Mapper(#[from] MapperError),

    #[error(transparent)]
    ThinEdgeJson(#[from] c8y_translator_lib::json::CumulocityJsonError),

    #[error(transparent)]
    ThinEdgeJsonSerialization(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    ThinEdgeJsonParser(#[from] thin_edge_json::parser::ThinEdgeJsonParserError),

    #[error(transparent)]
    MessageSizeExceeded(#[from] SizeThresholdExceeded),
}
