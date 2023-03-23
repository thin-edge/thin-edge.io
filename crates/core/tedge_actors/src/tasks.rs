use crate::Actor;
use crate::Builder;
use crate::DynSender;
use crate::RuntimeError;
use crate::RuntimeRequest;
use crate::RuntimeRequestSink;
use async_trait::async_trait;
use std::fmt::Formatter;

/// A task to be spawn in the background
#[async_trait]
pub trait Task: 'static + Send + Sync {
    fn name(&self) -> &str;

    fn runtime_request_sender(&self) -> DynSender<RuntimeRequest>;

    async fn run(self: Box<Self>) -> Result<(), RuntimeError>;
}

impl std::fmt::Debug for Box<dyn Task> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A task that run an actor
pub struct RunActor<A: Actor> {
    name: String,
    actor: A,
    runtime_request_sender: DynSender<RuntimeRequest>,
}

impl<A: Actor> RunActor<A> {
    pub(crate) fn new(actor: A, runtime_request_sender: DynSender<RuntimeRequest>) -> Self {
        let name = format!("actor '{}'", actor.name());
        RunActor {
            name,
            actor,
            runtime_request_sender,
        }
    }

    pub fn try_new<T>(actor_builder: T) -> Result<Self, RuntimeError>
    where
        T: Builder<A> + RuntimeRequestSink,
    {
        let runtime_request_sender: DynSender<RuntimeRequest> = actor_builder.get_signal_sender();
        let actor = actor_builder.build();
        let run_actor = RunActor::new(actor, runtime_request_sender);
        Ok(run_actor)
    }
}

#[async_trait]
impl<A: Actor> Task for RunActor<A> {
    fn name(&self) -> &str {
        &self.name
    }

    fn runtime_request_sender(&self) -> DynSender<RuntimeRequest> {
        self.runtime_request_sender.clone()
    }

    async fn run(mut self: Box<Self>) -> Result<(), RuntimeError> {
        let actor = self.actor;

        Ok(actor.run().await?)
    }
}
