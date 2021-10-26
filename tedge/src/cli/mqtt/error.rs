#[derive(thiserror::Error, Debug)]
pub enum MqttError {
    #[error("MQTT error")]
    FromRumqttClient(#[from] rumqttc::ClientError),

    #[error("I/O error")]
    FromIo(#[from] std::io::Error),

    #[error("Received message is not UTF-8 format")]
    FromUtf8(#[from] std::str::Utf8Error),

    #[error("The input QoS should be 0, 1, or 2")]
    InvalidQoS,

    #[error("MQTT connection error: {0}\n\nHint: Is MQTT server running?")]
    ServerConnection(String),
}
