use crate::size_threshold::SizeThresholdExceeded;
use mqtt_client::MqttClientError;
use tedge_config::TEdgeConfigError;
use thin_edge_json::serialize::ThinEdgeJsonSerializationError;

#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error(transparent)]
    MqttClientError(#[from] MqttClientError),

    #[error(
        "tedge_mapper accepts only one argument. Run `tedge_mapper c8y` or `tedge_mapper az`."
    )]
    IncorrectArgument,

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
    MapperError(#[from] MapperError),

    #[error(transparent)]
    ThinEdgeJsonError(#[from] c8y_translator_lib::json::CumulocityJsonError),

    #[error(transparent)]
    ThinEdgeJsonSerializationError(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    ThinEdgeJsonParserError(
        #[from] thin_edge_json::json::ThinEdgeJsonParserError<ThinEdgeJsonSerializationError>,
    ),

    #[error(transparent)]
    MessageSizeExceededError(#[from] SizeThresholdExceeded),
}
