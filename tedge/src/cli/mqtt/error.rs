#[derive(thiserror::Error, Debug)]
pub enum MqttError {
    #[error("MQTT error")]
    Connect(#[from] rumqttc::ClientError),

    #[error("I/O error")]
    Io(#[from] std::io::Error),

    #[error("Received message is not UTF-8 format")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("The input QoS should be 0, 1, or 2")]
    InvalidQoS,
}
