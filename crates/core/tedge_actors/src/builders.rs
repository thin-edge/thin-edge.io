use crate::{LinkError, Recipient, RuntimeError, RuntimeHandle};
use async_trait::async_trait;

/// Materialize an actor instance under construction
///
/// Such an instance is:
/// 1. built from some actor configuration
/// 2. connected to other peers
/// 3. eventually spawned into an actor.
#[async_trait]
pub trait ActorBuilder {
    /// Build and spawn the actor
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError>;
}

/// Implemented by actor builders to connect the actors under construction
pub trait PeerLinker<Input, Output> {
    fn connect(&mut self, output_sender: Recipient<Output>) -> Result<Recipient<Input>, LinkError>;
}
