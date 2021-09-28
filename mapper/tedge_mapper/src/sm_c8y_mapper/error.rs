use json_sm::SoftwareUpdateResponse;

#[derive(thiserror::Error, Debug)]
pub(crate) enum SmartRestSerializerError {
    #[error("The operation status is not supported. {response:?}")]
    UnsupportedOperationStatus { response: SoftwareUpdateResponse },

    #[error("Failed to serialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error(transparent)]
    FromCsvWriter(#[from] csv::IntoInnerError<csv::Writer<Vec<u8>>>),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum SmartRestDeserializerError {
    #[error("The received SmartREST message ID {id} is unsupported.")]
    UnsupportedOperation { id: String },

    #[error("Failed to deserialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error("Jwt response contains incorrect ID: {0}")]
    InvalidMessageId(u16),

    #[error("Action {action} is not recognized. It must be install or delete.")]
    ActionNotFound { action: String },
}

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
}
