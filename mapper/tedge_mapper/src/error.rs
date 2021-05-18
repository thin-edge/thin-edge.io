use tedge_config::TEdgeConfigError;
use thin_edge_json::serialize::ThinEdgeJsonSerializationError;

#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error(transparent)]
    MqttClientError(#[from] mqtt_client::Error),

    #[error("tedge_mapper accepts only one argument. Run `tedge_mapper c8y` or `tedge_mapper az`")]
    IncorrectArgument,

    #[error("The message size is too big. Must be smaller than {threshold} KB.")]
    MessageSizeError { threshold: usize },

    #[error("Home directory is not found.")]
    HomeDirNotFound,

    #[error(transparent)]
    TEdgeConfigError(#[from] TEdgeConfigError),

    #[error(transparent)]
    ConfigSettingError(#[from] tedge_config::ConfigSettingError),
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
}
