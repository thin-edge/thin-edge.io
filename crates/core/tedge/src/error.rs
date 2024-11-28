use tedge_config::MultiError;

#[derive(thiserror::Error, Debug)]
#[allow(clippy::enum_variant_names)]
pub enum TEdgeError {
    #[error("TOML parse error")]
    FromTomlParse(#[from] toml::de::Error),

    #[error("TOML serialization error")]
    FromInvalidToml(#[from] toml::ser::Error),

    #[error("I/O error")]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromPaths(#[from] tedge_utils::paths::PathsError),

    #[error(transparent)]
    FromTEdgeConfig(#[from] tedge_config::TEdgeConfigError),

    #[error(transparent)]
    FromTEdgeConfigSetting(#[from] tedge_config::ConfigSettingError),

    #[error(transparent)]
    FromRumqttClient(#[from] rumqttc::ClientError),

    #[error(transparent)]
    FromSystemServiceError(#[from] tedge_config::system_services::SystemServiceError),

    #[error(transparent)]
    FromTEdgeConfigRead(#[from] tedge_config::ReadError),

    #[error(transparent)]
    FromConfigNotSet(#[from] tedge_config::ConfigNotSet),

    #[error(transparent)]
    FromMultiError(#[from] MultiError),

    #[error(transparent)]
    FromCredentialsFileError(#[from] c8y_api::http_proxy::CredentialsFileError),

    #[error(transparent)]
    FromAnyhow(#[from] anyhow::Error),

    #[error(transparent)]
    FromC8yEndPointConfigError(#[from] c8y_api::http_proxy::C8yEndPointConfigError),
}
