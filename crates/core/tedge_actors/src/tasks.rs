use crate::Actor;
use crate::RuntimeError;
use async_trait::async_trait;
use std::fmt::Formatter;

/// A task to be spawn in the background
#[async_trait]
pub trait Task: 'static + Send + Sync {
    fn name(&self) -> &str;
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
    messages: A::MessageBox,
}

impl<A: Actor> RunActor<A> {
    pub fn new(actor: A, messages: A::MessageBox) -> Self {
        let name = format!("actor '{}'", actor.name());
        RunActor {
            name,
            actor,
            messages,
        }
    }
}

#[async_trait]
impl<A: Actor> Task for RunActor<A> {
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(mut self: Box<Self>) -> Result<(), RuntimeError> {
        let actor = self.actor;
        let messages = self.messages;

        Ok(actor.run(messages).await?)
    }
}
