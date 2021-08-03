use json_sm::error::SoftwareError;
use mqtt_client::MqttClientError;
use tedge_users::UserSwitchError;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),

    #[error(transparent)]
    MqttClient(#[from] MqttClientError),

    #[error("Couldn't load plugins from /etc/tedge/sm-plugins")]
    NoPlugins,

    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    SoftwareError(#[from] SoftwareError),

    #[error(transparent)]
    State(#[from] StateError),

    #[error(transparent)]
    UserSwitchError(#[from] UserSwitchError),
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error(transparent)]
    TOMLParseError(#[from] toml::de::Error),

    #[error(transparent)]
    InvalidTOMLError(#[from] toml::ser::Error),

    #[error(transparent)]
    IOError(#[from] std::io::Error),

    #[error("State file not found in /etc/tedge/.state")]
    FileNotFound,
}
