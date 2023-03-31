use tedge_api::serialize::ThinEdgeJsonSerializationError;
use tedge_mqtt_ext::MqttError;

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error(transparent)]
    FromThinEdgeJsonSerialization(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    FromThinEdgeJsonEventDeserialization(
        #[from] tedge_api::event::error::ThinEdgeJsonDeserializerError,
    ),

    #[error(transparent)]
    FromThinEdgeJsonParser(#[from] tedge_api::parser::ThinEdgeJsonParserError),

    #[error("The size of the message received on {topic} is {actual_size} which is greater than the threshold size of {threshold}.")]
    SizeThresholdExceeded {
        topic: String,
        actual_size: usize,
        threshold: usize,
    },

    #[error(transparent)]
    MqttError(#[from] MqttError),
}
