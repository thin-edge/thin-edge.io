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

    /// Create a new instance of this actor
    fn try_new(config: Self::Config) -> Result<Self, RuntimeError>;

    /// Start the actor
    async fn start(
        &mut self,
        runtime: RuntimeHandler,
        output: Recipient<Self::Output>,
    ) -> Result<(), RuntimeError>;

    /// React to an input message,
    /// possibly generating output messages and returning a message source
    async fn react(
        &mut self,
        message: Self::Input,
        runtime: &mut RuntimeHandler,
        output: &mut Recipient<Self::Output>,
    ) -> Result<(), RuntimeError>;
}

/// An handle to an inactive actor instance
///
/// An inactive instance encapsulates the actor config with a mailbox and connection to peers.
pub struct ActorInstance<A: Actor> {
    pub config: A::Config,
    pub mailbox: MailBox<A::Input>,
    pub recipient: Recipient<A::Output>,
}

/// Build a new actor instance with an address
///
/// The output of this instance will have to be connected to other actors using their addresses.
pub fn instance<A: Actor>(config: A::Config) -> ActorInstance<A> {
    let mailbox = MailBox::new();
    let recipient = Box::new(DevNull);

    ActorInstance {
        config,
        mailbox,
        recipient,
    }
}

impl<A: Actor> ActorInstance<A> {
    /// Return the address of this actor
    pub fn address(&self) -> Address<A::Input> {
        self.mailbox.get_address()
    }

    /// Update the messages recipient for this actor.
    pub fn set_recipient(&mut self, recipient: Recipient<A::Output>) {
        self.recipient = recipient;
    }

    pub async fn run(self, runtime: &mut Runtime) -> Result<ActiveActor<A>, RuntimeError> {
        runtime.run(self).await
    }
}

/// An handle to an active actor
///
pub struct ActiveActor<A: Actor> {
    pub input: Address<A::Input>,
}

/// A vector can be used as a message source - mostly useful for tests
struct VecSource<M: Message> {
    messages: Vec<M>,
    output: Recipient<M>,
}

#[async_trait]
impl<M: Message> Task for VecSource<M> {
    async fn run(mut self: Box<Self>) -> Result<(), RuntimeError> {
        for message in self.messages.into_iter() {
            self.output.send_message(message.clone()).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl<M: Message> Actor for Vec<M> {
    type Config = Vec<M>;
    type Input = NoMessage;
    type Output = M;

    fn try_new(config: Self::Config) -> Result<Self, RuntimeError> {
        Ok(config)
    }

    async fn start(
        &mut self,
        mut runtime: RuntimeHandler,
        output: Recipient<Self::Output>,
    ) -> Result<(), RuntimeError> {
        runtime
            .spawn(VecSource {
                messages: self.clone(),
                output,
            })
            .await
    }

    async fn react(
        &mut self,
        _message: Self::Input,
        _runtime: &mut RuntimeHandler,
        _output: &mut Recipient<Self::Output>,
    ) -> Result<(), RuntimeError> {
        Ok(())
    }
}
