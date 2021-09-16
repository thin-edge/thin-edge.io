#[derive(thiserror::Error, Debug)]
pub enum MqttError {
    #[error("MQTT error")]
    ConnectError(#[from] rumqttc::ClientError),

    #[error("I/O error")]
    IoError(#[from] std::io::Error),

    #[error("Received message is not UTF-8 format")]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error("The input QoS should be 0, 1, or 2")]
    InvalidQoSError,

    #[error("MQTT connection error: {0}\n\nHint: Is MQTT server running?")]
    ServerError(String),
}
