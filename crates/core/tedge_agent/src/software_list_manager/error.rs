#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum SoftwareListManagerError {
    #[error("Couldn't load plugins from {plugins_path}")]
    NoPlugins { plugins_path: camino::Utf8PathBuf },

    #[error(transparent)]
    FromChannelError(#[from] tedge_actors::ChannelError),

    #[error(transparent)]
    FromState(#[from] crate::state_repository::error::StateError),

    #[error(transparent)]
    FromOperationsLogs(#[from] plugin_sm::operation_logs::OperationLogsError),
}

impl From<SoftwareListManagerError> for tedge_actors::RuntimeError {
    fn from(error: SoftwareListManagerError) -> Self {
        tedge_actors::RuntimeError::ActorError(Box::new(error))
    }
}
