use tedge_actors::RuntimeError;

#[derive(Debug, thiserror::Error)]
#[allow(clippy::enum_variant_names)]
pub enum AgentError {
    #[error(transparent)]
    FromFlockfileError(#[from] flockfile::FlockfileError),
}

impl From<AgentError> for RuntimeError {
    fn from(error: AgentError) -> Self {
        RuntimeError::ActorError(Box::new(error))
    }
}
