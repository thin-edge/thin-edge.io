use std::io;
use tedge_actors::RuntimeError;
use tedge_mqtt_ext::Topic;
use tedge_utils::file::FileError;
use tedge_utils::paths::PathsError;

#[derive(thiserror::Error, Debug)]
pub enum ConfigManagementError {
    #[error(transparent)]
    InvalidRequestedConfigType(#[from] super::plugin_config::InvalidConfigTypeError),

    #[error(transparent)]
    InvalidChildDeviceTopic(#[from] super::child_device::InvalidChildDeviceTopicError),

    #[error("Invalid operation response with empty status received on topic: {0:?}")]
    EmptyOperationStatus(Topic),

    #[error(transparent)]
    FromFile(#[from] FileError),

    #[error(transparent)]
    FromIoError(#[from] io::Error),

    #[error(transparent)]
    FromMqttError(#[from] tedge_mqtt_ext::MqttError),

    #[error(transparent)]
    FromSmartRestSerializerError(#[from] c8y_api::smartrest::error::SmartRestSerializerError),

    #[error("Failed to parse response from child device with: {0}")]
    FromSerdeJsonError(#[from] serde_json::Error),

    #[error(transparent)]
    FromChannelError(#[from] tedge_actors::ChannelError),

    #[error(transparent)]
    FromC8YRestError(#[from] c8y_http_proxy::messages::C8YRestError),

    #[error(transparent)]
    FromPathsError(#[from] PathsError),
}

impl From<ConfigManagementError> for RuntimeError {
    fn from(error: ConfigManagementError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}
