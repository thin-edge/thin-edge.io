use agent_interface::SoftwareUpdateResponse;
use json_writer::JsonWriterError;
use std::path::PathBuf;

// allowing large size difference between variants warning,
// because the enum `SmartRestSerializerError` is already Boxed
// in `SMCumulocityMapperError`
#[allow(clippy::large_enum_variant)]
#[derive(thiserror::Error, Debug)]
pub enum SmartRestSerializerError {
    #[error("The operation status is not supported. {response:?}")]
    UnsupportedOperationStatus { response: SoftwareUpdateResponse },

    #[error("Failed to serialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error(transparent)]
    FromCsvWriter(#[from] csv::IntoInnerError<csv::Writer<Vec<u8>>>),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    FromTimeFormatError(#[from] time::error::Format),
}

#[derive(thiserror::Error, Debug)]
pub enum SmartRestDeserializerError {
    #[error("The received SmartREST message ID {id} is unsupported.")]
    UnsupportedOperation { id: String },

    #[error("Failed to deserialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error("Jwt response contains incorrect ID: {0}")]
    InvalidMessageId(u16),

    #[error("Parameter {parameter} is not recognized. {hint}")]
    InvalidParameter {
        operation: String,
        parameter: String,
        hint: String,
    },

    #[error("Empty request")]
    EmptyRequest,
}

#[derive(Debug, thiserror::Error)]
pub enum OperationsError {
    #[error("Failed to read directory: {dir}")]
    ReadDirError { dir: PathBuf },

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Cannot extract the operation name from the path: {0}")]
    InvalidOperationName(PathBuf),

    #[error("Error while parsing operation file: '{0}': {1}.")]
    TomlError(PathBuf, #[source] toml::de::Error),
}

#[derive(thiserror::Error, Debug)]
pub enum SMCumulocityMapperError {
    #[error("Invalid MQTT Message.")]
    InvalidMqttMessage,

    #[error(transparent)]
    InvalidTopicError(#[from] agent_interface::TopicError),

    #[error(transparent)]
    InvalidThinEdgeJson(#[from] agent_interface::SoftwareError),

    #[error(transparent)]
    FromElapsed(#[from] tokio::time::error::Elapsed),

    #[error(transparent)]
    FromMqttClient(#[from] mqtt_channel::MqttError),

    #[error(transparent)]
    FromReqwest(#[from] reqwest::Error),

    #[error(transparent)]
    FromSmartRestSerializer(#[from] Box<SmartRestSerializerError>),

    #[error(transparent)]
    FromSmartRestDeserializer(#[from] SmartRestDeserializerError),

    #[error(transparent)]
    FromTedgeConfig(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromLoadTedgeConfigError(#[from] tedge_config::TEdgeConfigError),

    #[error("Invalid date in file name: {0}")]
    InvalidDateInFileName(String),

    #[error("Invalid path. Not UTF-8.")]
    InvalidUtf8Path,

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("Request timed out")]
    RequestTimeout,

    #[error("Operation execution failed: {0}")]
    ExecuteFailed(String),

    #[error("An unknown operation template: {0}")]
    UnknownOperation(String),

    #[error(transparent)]
    FromTimeFormat(#[from] time::error::Format),

    #[error(transparent)]
    FromTimeParse(#[from] time::error::Parse),

    #[error(transparent)]
    JsonWriterError(#[from] JsonWriterError),

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),
}
