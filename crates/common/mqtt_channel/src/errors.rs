/// An MQTT related error
#[derive(thiserror::Error, Debug)]
pub enum MqttError {
    #[error("Invalid topic name: {name:?}")]
    InvalidTopic { name: String },

    #[error("Invalid topic filter: {pattern:?}")]
    InvalidFilter { pattern: String },

    #[error("Invalid session: a session name must be provided")]
    InvalidSessionConfig,

    #[error("MQTT client error: {0}")]
    ClientError(#[from] rumqttc::ClientError),

    #[error("MQTT connection error: {0}")]
    ConnectionError(#[from] rumqttc::ConnectionError),

    #[error("Invalid UTF8 payload: {from}: {input_excerpt}...")]
    InvalidUtf8Payload {
        input_excerpt: String,
        from: std::str::Utf8Error,
    },

    #[error(
        "The read channel of the connection has been closed and no more messages can be received"
    )]
    ReadOnClosedConnection,

    #[error(
        "The send channel of the connection has been closed and no more messages can be published"
    )]
    SendOnClosedConnection,
}

impl MqttError {
    pub fn new_invalid_utf8_payload(bytes: &[u8], from: std::str::Utf8Error) -> MqttError {
        const EXCERPT_LEN: usize = 80;
        let index = from.valid_up_to();
        let input = std::str::from_utf8(&bytes[..index]).unwrap_or("");

        MqttError::InvalidUtf8Payload {
            input_excerpt: MqttError::input_prefix(input, EXCERPT_LEN),
            from,
        }
    }

    fn input_prefix(input: &str, len: usize) -> String {
        input
            .chars()
            .filter(|c| !c.is_whitespace())
            .take(len)
            .collect()
    }
}

impl From<futures::channel::mpsc::SendError> for MqttError {
    fn from(_: futures::channel::mpsc::SendError) -> Self {
        MqttError::SendOnClosedConnection
    }
}
