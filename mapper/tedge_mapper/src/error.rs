#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error(transparent)]
    MqttClientError(#[from] mqtt_client::Error),

    #[error("tedge_mapper accepts only one argument. Run `tedge_mapper c8y` or `tedge_mapper az`")]
    IncorrectArgument,
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error(transparent)]
    ThinEdgeJsonError(#[from] thin_edge_json::json::ThinEdgeJsonError),

    #[error(transparent)]
    AzureMapperError(#[from] crate::az_mapper::AzureMapperError),

    #[error(transparent)]
    ThinEdgeJsonParserError(#[from] thin_edge_json::json::ThinEdgeJsonParserError<thin_edge_json::json::ThinEdgeJsonError>)
}
