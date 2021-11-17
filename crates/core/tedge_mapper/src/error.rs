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
    FromMapperError(#[from] MapperError),

    #[error(transparent)]
    FromCumulocityJsonError(#[from] c8y_translator::json::CumulocityJsonError),

    #[error(transparent)]
    FromThinEdgeJsonSerializationError(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    FromThinEdgeJsonParserError(#[from] thin_edge_json::parser::ThinEdgeJsonParserError),

    #[error(transparent)]
    SizeThresholdExceeded(#[from] SizeThresholdExceeded),
}
