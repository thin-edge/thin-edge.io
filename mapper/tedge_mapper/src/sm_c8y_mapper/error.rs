use json_sm::SoftwareUpdateResponse;

#[derive(thiserror::Error, Debug)]
pub(crate) enum SmartRestSerializerError {
    #[error("The operation status is not executing. {response:?}")]
    StatusIsNotExecuting { response: SoftwareUpdateResponse },

    #[error("The operation status is not successful. {response:?}")]
    StatusIsNotSuccessful { response: SoftwareUpdateResponse },

    #[error("The operation status is not failed. {response:?}")]
    StatusIsNotFailed { response: SoftwareUpdateResponse },

    #[error("Failed to serialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error(transparent)]
    FromCsvWriter(#[from] csv::IntoInnerError<csv::Writer<Vec<u8>>>),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum SmartRestDeserializerError {
    #[error("The SmartREST message ID is not Update Software Operation (528).")]
    NotUpdateSoftwareOperation,

    #[error("Failed to deserialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error(
        "Action {action} is not recognized. Acceptable software actions are install or delete."
    )]
    ActionNotFound { action: String },
}

#[derive(thiserror::Error, Debug)]
pub(crate) enum SMCumulocityMapperError {
    #[error(transparent)]
    InvalidThinEdgeJson(#[from] json_sm::SoftwareError),

    #[error(transparent)]
    FromMqttClient(#[from] mqtt_client::MqttClientError),

    #[error(transparent)]
    FromSmartRestSerializer(#[from] SmartRestSerializerError),

    #[error(transparent)]
    FromSmartRestDeserializer(#[from] SmartRestDeserializerError),
}
