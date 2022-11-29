use crate::{Actor, RuntimeError};
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
    actor: A,
    mailbox: A::Mailbox,
    peers: A::Peers,
}

impl<A: Actor> RunActor<A> {
    pub fn new(actor: A, mailbox: A::Mailbox, peers: A::Peers) -> Self {
        RunActor {
            actor,
            mailbox,
            peers,
        }
    }
}

#[async_trait]
impl<A: Actor> Task for RunActor<A> {
    fn name(&self) -> &str {
        // TODO: Assign an instance id to each actor and used it here
        //       eg: c8y-mapper or mqtt#12
        "actor"
    }

    async fn run(mut self: Box<Self>) -> Result<(), RuntimeError> {
        let actor = self.actor;
        let mailbox = self.mailbox;
        let peers = self.peers;

        Ok(actor.run(mailbox, peers).await?)
    }
}
