use tedge_mqtt_ext::MqttError;

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("The size of the message received on {topic} is {actual_size} which is greater than the threshold size of {threshold}.")]
    SizeThresholdExceeded {
        topic: String,
        actual_size: usize,
        threshold: usize,
    },

    #[error(transparent)]
    MqttError(#[from] MqttError),

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    FromTimeFormatError(#[from] time::error::Format),
}
