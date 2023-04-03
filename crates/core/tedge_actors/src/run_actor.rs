use crate::Actor;
use crate::Builder;
use crate::DynSender;
use crate::RuntimeError;
use crate::RuntimeRequest;
use crate::RuntimeRequestSink;
use std::fmt::Debug;
use std::fmt::Formatter;

/// Holds an Actor and its associated RuntimeRequest sender
pub struct RunActor {
    actor: Box<dyn Actor>,
    runtime_request_sender: DynSender<RuntimeRequest>,
}

impl RunActor {
    pub(crate) fn new(
        actor: Box<dyn Actor>,
        runtime_request_sender: DynSender<RuntimeRequest>,
    ) -> Self {
        RunActor {
            actor,
            runtime_request_sender,
        }
    }

    pub fn from_builder<A, T>(actor_builder: T) -> Self
    where
        A: Actor,
        T: Builder<A> + RuntimeRequestSink,
    {
        let runtime_request_sender = actor_builder.get_signal_sender();
        let actor = actor_builder.build();
        RunActor::new(Box::new(actor), runtime_request_sender)
    }

    pub fn name(&self) -> &str {
        self.actor.name()
    }

    pub async fn run(mut self) -> Result<(), RuntimeError> {
        self.actor.run().await
    }
}

impl Debug for RunActor {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        f.write_str(self.actor.name())
    }
}

impl RuntimeRequestSink for RunActor {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.runtime_request_sender.sender_clone()
    }
}
