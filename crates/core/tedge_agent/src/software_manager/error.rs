#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum SoftwareManagerError {
    #[error("Couldn't load plugins from {plugins_path}")]
    NoPlugins { plugins_path: camino::Utf8PathBuf },

    #[error(transparent)]
    FromChannelError(#[from] tedge_actors::ChannelError),

    #[error(transparent)]
    FromState(#[from] crate::state_repository::error::StateError),

    #[error(transparent)]
    FromOperationsLogs(#[from] plugin_sm::operation_logs::OperationLogsError),

    #[error(transparent)]
    FromIo(#[from] std::io::Error),

    #[error(transparent)]
    FromSoftware(#[from] tedge_api::SoftwareError),

    #[error(transparent)]
    FromTedgeConfig(#[from] tedge_config::TEdgeConfigError),
}

impl From<SoftwareManagerError> for tedge_actors::RuntimeError {
    fn from(error: SoftwareManagerError) -> Self {
        tedge_actors::RuntimeError::ActorError(Box::new(error))
    }
}
