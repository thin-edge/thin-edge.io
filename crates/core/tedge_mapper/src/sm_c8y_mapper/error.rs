use c8y_smartrest::error::{SmartRestDeserializerError, SmartRestSerializerError};

#[derive(thiserror::Error, Debug)]
pub(crate) enum MapperTopicError {
    #[error("Topic {topic} is unknown.")]
    UnknownTopic { topic: String },
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum SMCumulocityMapperError {
    #[error("Invalid MQTT Message.")]
    InvalidMqttMessage,

    #[error(transparent)]
    InvalidTopicError(#[from] MapperTopicError),

    #[error(transparent)]
    InvalidThinEdgeJson(#[from] json_sm::SoftwareError),

    #[error(transparent)]
    FromElapsed(#[from] tokio::time::error::Elapsed),

    #[error(transparent)]
    FromMqttClient(#[from] mqtt_client::MqttClientError),

    #[error(transparent)]
    FromReqwest(#[from] reqwest::Error),

    #[error(transparent)]
    FromSmartRestSerializer(#[from] SmartRestSerializerError),

    #[error(transparent)]
    FromSmartRestDeserializer(#[from] SmartRestDeserializerError),

    #[error(transparent)]
    FromTedgeConfig(#[from] tedge_config::ConfigSettingError),

    #[error("Invalid date in file name: {0}")]
    InvalidDateInFileName(String),

    #[error("Invalid path. Not UTF-8.")]
    InvalidUtf8Path,

    #[error(transparent)]
    FromChronoParse(#[from] chrono::ParseError),

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Request timed out")]
    RequestTimeout,
}
