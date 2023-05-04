use rumqttc::tokio_rustls::rustls;
use rumqttc::ConnectReturnCode;

/// An MQTT related error
#[derive(thiserror::Error, Debug)]
pub enum MqttError {
    #[error("Invalid topic name: {name:?}")]
    InvalidTopic { name: String },

    #[error("Invalid topic filter: {pattern:?}")]
    InvalidFilter { pattern: String },

    #[error("Invalid session: a session name must be provided")]
    InvalidSessionConfig,

    #[error(transparent)]
    InvalidPrivateKey(#[from] rustls::Error),

    #[error("MQTT client error: {0}")]
    ClientError(#[from] rumqttc::ClientError),

    #[error("MQTT connection error: {0}")]
    ConnectionError(#[from] rumqttc::ConnectionError),

    #[error("MQTT connection rejected: {0:?}")]
    ConnectionRejected(rumqttc::ConnectReturnCode),

    #[error("MQTT subscription failure")]
    // The MQTT specs are mysterious on the possible cause of such a failure
    SubscriptionFailure,

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

    #[error("Failed to create a TLS config")]
    TlsConfig(#[from] certificate::CertificateError),

    #[error("Failed to initialize the session with MQTT Broker due to: {reason} ")]
    InitSessionError { reason: String },
}

impl MqttError {
    pub fn maybe_connection_error(ack: &rumqttc::ConnAck) -> Option<MqttError> {
        match ack.code {
            rumqttc::ConnectReturnCode::Success => None,
            err => Some(MqttError::ConnectionRejected(err)),
        }
    }

    pub fn maybe_subscription_error(ack: &rumqttc::SubAck) -> Option<MqttError> {
        for code in ack.return_codes.iter() {
            if let rumqttc::SubscribeReasonCode::Failure = code {
                return Some(MqttError::SubscriptionFailure);
            }
        }
        None
    }

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

    pub fn from_connection_error(err: rumqttc::ConnectionError) -> MqttError {
        match err {
            rumqttc::ConnectionError::ConnectionRefused(ConnectReturnCode::BadClientId) => {
                MqttError::InitSessionError {
                    reason: "bad client id".to_string(),
                }
            }
            rumqttc::ConnectionError::ConnectionRefused(ConnectReturnCode::BadUserNamePassword) => {
                MqttError::InitSessionError {
                    reason: "bad user name and password".to_string(),
                }
            }
            rumqttc::ConnectionError::ConnectionRefused(ConnectReturnCode::NotAuthorized) => {
                MqttError::InitSessionError {
                    reason: " not authorized".to_string(),
                }
            }
            rumqttc::ConnectionError::ConnectionRefused(
                ConnectReturnCode::RefusedProtocolVersion,
            ) => MqttError::InitSessionError {
                reason: " protocol version mismatch".to_string(),
            },
            rumqttc::ConnectionError::ConnectionRefused(ConnectReturnCode::ServiceUnavailable) => {
                MqttError::InitSessionError {
                    reason: " service not available".to_string(),
                }
            }
            rumqttc::ConnectionError::ConnectionRefused(ConnectReturnCode::Success) => {
                MqttError::InitSessionError {
                    reason: "Connection successful".to_string(),
                }
            }
            e => MqttError::InitSessionError {
                reason: e.to_string(),
            },
        }
    }
}

impl From<futures::channel::mpsc::SendError> for MqttError {
    fn from(_: futures::channel::mpsc::SendError) -> Self {
        MqttError::SendOnClosedConnection
    }
}
