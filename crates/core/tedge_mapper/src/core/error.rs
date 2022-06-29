use crate::c8y::error::CumulocityMapperError;

use c8y_smartrest::error::OperationsError;
use mqtt_channel::MqttError;
use tedge_config::TEdgeConfigError;
use thin_edge_json::serialize::ThinEdgeJsonSerializationError;

// allowing enum_variant_names due to a False positive where it is
// detected that "all variants have the same prefix: `From`"
#[allow(clippy::enum_variant_names)]
#[derive(Debug, thiserror::Error)]
pub enum MapperError {
    #[error(transparent)]
    FromMqttClient(#[from] MqttError),

    #[cfg(test)] // this error is only used in a test so far
    #[error("Home directory is not found.")]
    HomeDirNotFound,

    #[error(transparent)]
    FromTEdgeConfig(#[from] TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromFlockfile(#[from] flockfile::FlockfileError),
}

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error(transparent)]
    FromMapper(#[from] MapperError),

    #[error(transparent)]
    FromCumulocityJsonError(#[from] c8y_translator::json::CumulocityJsonError),

    #[error(transparent)]
    FromCumulocityMapperError(#[from] CumulocityMapperError),

    #[error(transparent)]
    FromCumulocitySmartRestMapperError(#[from] c8y_smartrest::error::SMCumulocityMapperError),

    #[error(transparent)]
    FromThinEdgeJsonSerialization(#[from] ThinEdgeJsonSerializationError),

    #[error(transparent)]
    FromThinEdgeJsonAlarmDeserialization(
        #[from] thin_edge_json::alarm::ThinEdgeJsonDeserializerError,
    ),

    #[error(transparent)]
    FromThinEdgeJsonEventDeserialization(
        #[from] thin_edge_json::event::error::ThinEdgeJsonDeserializerError,
    ),

    #[error(transparent)]
    FromThinEdgeJsonParser(#[from] thin_edge_json::parser::ThinEdgeJsonParserError),

    #[error("The size of the message received on {topic} is {actual_size} which is greater than the threshold size of {threshold}.")]
    SizeThresholdExceeded {
        topic: String,
        actual_size: usize,
        threshold: usize,
    },

    #[error("The given Child ID '{id}' is invalid.")]
    InvalidChildId { id: String },

    #[error(transparent)]
    FromMqttClient(#[from] MqttError),

    #[error(transparent)]
    FromOperationsError(#[from] OperationsError),

    #[error(transparent)]
    FromSmartRestSerializerError(#[from] c8y_smartrest::error::SmartRestSerializerError),

    #[error("Unsupported topic: {0}")]
    UnsupportedTopic(String),

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    FromStdIo(#[from] std::io::Error),

    #[error("Error converting json option")]
    FromOptionError,

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    FromTimeFormatError(#[from] time::error::Format),

    #[error("The payload {payload} received on {topic} after translation is {actual_size} greater than the threshold size of {threshold}.")]
    TranslatedSizeExceededThreshold {
        payload: String,
        topic: String,
        actual_size: usize,
        threshold: usize,
    },

    #[error(transparent)]
    FromOperationLogsError(#[from] plugin_sm::operation_logs::OperationLogsError),

    #[error("The given Child ID '{id}' is not registered with Cumulocity. To send the events to the child device, it has to be registered first.")]
    ChildDeviceNotRegistered { id: String },
}
