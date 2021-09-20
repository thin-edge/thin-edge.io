use json_sm::SoftwareError;
use mqtt_client::MqttClientError;
use tedge_config::{ConfigSettingError, TEdgeConfigError};
use flockfile::FlockfileError;

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error("An internal task failed to complete.")]
    FromJoin(#[from] tokio::task::JoinError),

    #[error(transparent)]
    FromMqttClient(#[from] MqttClientError),

    #[error("Couldn't load plugins from /etc/tedge/sm-plugins")]
    NoPlugins,

    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    FromSoftware(#[from] SoftwareError),

    #[error(transparent)]
    FromState(#[from] StateError),

    #[error(transparent)]
    FromTedgeConfig(#[from] TEdgeConfigError),

    #[error(transparent)]
    FromConfigSetting(#[from] ConfigSettingError),

    #[error(transparent)]
    FromFlockfileError(#[from] FlockfileError),
}

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error(transparent)]
    FromTOMLParse(#[from] toml::de::Error),

    #[error(transparent)]
    FromInvalidTOML(#[from] toml::ser::Error),

    #[error(transparent)]
    FromIo(#[from] std::io::Error),
}
