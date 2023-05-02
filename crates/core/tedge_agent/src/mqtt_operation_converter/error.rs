use tedge_actors::RuntimeError;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum MqttRequestConverterError {
    #[error(transparent)]
    FromSerdeJson(#[from] serde_json::Error),

    #[error(transparent)]
    FromChannelError(#[from] tedge_actors::ChannelError),
}

impl From<MqttRequestConverterError> for RuntimeError {
    fn from(error: MqttRequestConverterError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}
