use crate::runtime::ActorRuntime;
use crate::*;
use async_trait::async_trait;

/// An actor is a state machine consuming & producing messages
///
/// An actor concurrently:
/// - consumes messages received from other actors,
/// - produces messages as a reaction to received messages,
/// - might maintain an internal state controlling its behaviour,
/// - might produce messages spontaneously.
#[async_trait]
pub trait Actor: 'static + Sized + Send + Sync {
    /// The configuration of an actor instance
    type Config;

    /// The type of input messages this actor consumes
    type Input: Message;

    /// The type of output messages this actor produces
    type Output: Message;

    /// The actual type of the source for spontaneous messages
    type Producer: Producer<Self::Output>;

    /// The actual type of the source for spontaneous messages
    type Reactor: Reactor<Self::Input, Self::Output>;

    /// Create a new instance of this actor
    fn try_new(config: &Self::Config) -> Result<Self, RuntimeError>;

    /// Start the actor returning a message source and a reactor
    async fn start(self) -> Result<(Self::Producer, Self::Reactor), RuntimeError>;
}

/// A state machine that reacts to input messages by producing output messages
#[async_trait]
pub trait Reactor<Input, Output>: 'static + Sized + Send + Sync {
    /// React to an input message, possibly generating output messages
    async fn react(
        &mut self,
        message: Input,
        output: &mut impl Recipient<Output>,
    ) -> Result<(), RuntimeError>;
}

/// An handle to an inactive actor instance
///
/// Such instances have each an address to be used to interconnect the actors.
pub struct ActorInstance<A: Actor, R: Recipient<A::Output>> {
    pub actor: A,
    pub mailbox: MailBox<A::Input>,
    pub recipient: R,
}

/// Build a new actor instance with an address
///
/// The output of this instance will have to be connected to other actors using their addresses.
pub fn instance<A: Actor>(config: &A::Config) -> Result<ActorInstance<A, DevNull>, RuntimeError> {
    let actor = A::try_new(config)?;
    let mailbox = MailBox::new();
    let recipient = DevNull;

    Ok(ActorInstance {
        actor,
        mailbox,
        recipient,
    })
}

impl<A: Actor, R: Recipient<A::Output>> ActorInstance<A, R> {
    /// Return the address of this actor
    pub fn address(&self) -> Address<A::Input> {
        self.mailbox.get_address()
    }

    /// Update the messages recipient for this actor.
    pub fn with_recipient<S: Recipient<A::Output>>(self, recipient: S) -> ActorInstance<A, S> {
        ActorInstance {
            actor: self.actor,
            mailbox: self.mailbox,
            recipient,
        }
    }

    pub async fn run(self, runtime: &ActorRuntime) -> ActiveActor<A, R> {
        runtime.run(self).await
    }
}

/// An handle to an active actor
///
pub struct ActiveActor<A: Actor, R: Recipient<A::Output>> {
    pub input: Address<A::Input>,
    pub output: R,
}

/// A vector can be used as a message source as well as a recipient of messages - mostly useful for tests
#[async_trait]
impl<M: Message> Recipient<M> for std::sync::Arc<futures::lock::Mutex<Vec<M>>> {
    async fn send_message(&mut self, message: M) -> Result<(), RuntimeError> {
        let mut vec = self.lock().await;
        vec.push(message);
        Ok(())
    }
}

#[async_trait]
impl<M: Message> Producer<M> for Vec<M> {
    async fn produce_messages(self, mut output: impl Recipient<M>) -> Result<(), RuntimeError> {
        for message in self.into_iter() {
            output.send_message(message).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl<M: Message> Actor for Vec<M> {
    type Config = Vec<M>;
    type Input = NoMessage;
    type Output = M;
    type Producer = Vec<M>;
    type Reactor = DevNull;

    fn try_new(config: &Self::Config) -> Result<Self, RuntimeError> {
        Ok(config.clone())
    }

    async fn start(self) -> Result<(Self::Producer, Self::Reactor), RuntimeError> {
        Ok((self, DevNull))
    }
}
