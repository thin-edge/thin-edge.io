use crate::runtime::ActorRuntime;
use crate::*;
use async_trait::async_trait;

/// An actor is a state machine consuming & producing messages
///
/// An actor concurrently
/// - consumes messages received from other actors,
/// - produces messages, either spontaneously or as a reaction to received messages,
/// - maintains its state.
#[async_trait]
pub trait Actor: Sized + Send + Sync {
    /// The configuration of an actor instance
    type Config;

    /// The type of input messages this actor consumes
    type Input: Message;

    /// The type of output messages this actor produces
    type Output: Message;

    /// The actual type of the source for spontaneous messages
    type Producer: Producer<Self::Output>;

    /// Create a new instance of this actor
    fn try_new(config: &Self::Config) -> Result<Self, RuntimeError>;

    /// Return the source for spontaneous output messages, aka events
    fn event_source(&self) -> Self::Producer;

    /// React to an input message, possibly generating output messages
    async fn react(
        &self,
        message: Self::Input,
        output: &mut impl Recipient<Self::Output>,
    ) -> Result<(), RuntimeError>;
}

/// An active actor ready to be started
pub struct ActiveActor<A: Actor, C: Recipient<A::Output>> {
    mailbox: MailBox<A::Input>,
    actor: A,
    recipient: C,
}

/// Build an actor instance ready to run
pub fn instantiate<A: Actor, C: Recipient<A::Output>>(
    config: &A::Config,
    recipient: C,
) -> Result<ActiveActor<A, C>, RuntimeError> {
    let actor = A::try_new(config)?;
    let mailbox = MailBox::new();
    Ok(ActiveActor {
        mailbox,
        actor,
        recipient,
    })
}

impl<A: Actor, C: Recipient<A::Output>> ActiveActor<A, C> {
    /// Return the address of this actor
    pub fn get_address(&self) -> Address<A::Input> {
        self.mailbox.get_address()
    }

    /// Run the actor, producing output messages and reacting to input messages
    pub fn run(&'static mut self, runtime: &ActorRuntime) {
        let event_source = self.actor.event_source();
        let recipient = self.recipient.clone();
        runtime.spawn(event_source.produce_messages(recipient));

        let mailbox = &mut self.mailbox;
        let recipient = &mut self.recipient;
        runtime.spawn(async {
            while let Some(message) = mailbox.next_message().await {
                self.actor.react(message, recipient).await?;
            }
            Ok(())
        });
    }
}
