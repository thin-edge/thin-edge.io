use c8y_smartrest::error::{SmartRestDeserializerError, SmartRestSerializerError};

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    // #[error(transparent)]
    // FromIo(#[from] std::io::Error),
    #[error("I/O error: {reason:?}")]
    FromIo { reason: String },

    #[error("JSON parse error: {reason:?}")]
    FromReqwest { reason: String },
    // #[error(transparent)]
    // FromReqwest(#[from] reqwest::Error),
    // #[error(transparent)]
    // InvalidThinEdgeJson(#[from] json_sm::SoftwareError),
    #[error("Invalid MQTT Message.")]
    InvalidMqttMessage,

    #[error(transparent)]
    FromElapsed(#[from] tokio::time::error::Elapsed),

    #[error(transparent)]
    FromMqttClient(#[from] mqtt_client::MqttClientError),

    #[error(transparent)]
    FromSmartRestSerializer(#[from] SmartRestSerializerError),

    #[error(transparent)]
    FromSmartRestDeserializer(#[from] SmartRestDeserializerError),

    #[error(transparent)]
    FromTedgeConfig(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromUrlParse(#[from] url::ParseError),

    #[error("Scheme {0} is not supported")]
    UnsupportedScheme(String),
}

impl From<reqwest::Error> for DownloadError {
    fn from(err: reqwest::Error) -> Self {
        DownloadError::FromReqwest {
            reason: format!("{}", err),
        }
    }
}

impl From<std::io::Error> for DownloadError {
    fn from(err: std::io::Error) -> Self {
        DownloadError::FromIo {
            reason: format!("{}", err),
        }
    }
}
