use std::io;
use std::path::PathBuf;
use tedge_api::SoftwareUpdateResponse;

// allowing large size difference between variants warning,
// because the enum `SmartRestSerializerError` is already Boxed
// in `SMCumulocityMapperError`
#[derive(thiserror::Error, Debug)]
pub enum SmartRestSerializerError {
    #[error("The operation status is not supported. {response:?}")]
    UnsupportedOperationStatus { response: SoftwareUpdateResponse },

    #[error("Failed to serialize SmartREST.")]
    InvalidCsv(#[from] csv::Error),

    #[error("IO error")]
    IoError(#[from] io::Error),

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

    #[error("No response")]
    NoResponse,
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
    InvalidTopicError(#[from] tedge_api::TopicError),

    #[error(transparent)]
    InvalidThinEdgeJson(#[from] tedge_api::SoftwareError),

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
    FromDownload(#[from] download::DownloadError),

    #[error("Error configuring MQTT client")]
    FromMqttConfigBuild(#[from] tedge_config::mqtt_config::MqttConfigBuildError),
}
