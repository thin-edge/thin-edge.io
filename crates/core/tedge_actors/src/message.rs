use crate::*;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::fmt::Debug;

/// A message exchanged between two actors
pub trait Message: 'static + Clone + Debug + Send + Sync {}

/// A recipient for messages of type `M`
#[async_trait]
pub trait Recipient<M>: 'static + Clone + Debug + Send + Sync {
    async fn send_message(&mut self, message: M) -> Result<(), RuntimeError>;
}

/// An address where messages of type `Into<M>` can be sent
#[derive(Clone, Debug)]
pub struct Address<M> {
    sender: mpsc::UnboundedSender<M>,
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Recipient<N> for Address<M> {
    async fn send_message(&mut self, message: N) -> Result<(), RuntimeError> {
        Ok(self.sender.send(message.into()).await?)
    }
}

/// A mailbox gathering all the messages to be processed by an actor
pub struct MailBox<M> {
    sender: mpsc::UnboundedSender<M>,
    receiver: mpsc::UnboundedReceiver<M>,
}

impl<M> MailBox<M> {
    /// Build a new mailbox
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded();
        MailBox { sender, receiver }
    }

    /// Return the next message if any
    ///
    /// Return `None` if all the pending messages have been consumed
    /// and all the senders have been closed.
    pub async fn next_message(&mut self) -> Option<M> {
        self.receiver.next().await
    }

    /// Return the address of this mailbox
    pub fn get_address(&self) -> Address<M> {
        Address {
            sender: self.sender.clone(),
        }
    }
}

/// A source of messages
#[async_trait]
pub trait Producer<M: Message> {
    /// Produce the messages of this source sending them to the given recipient
    async fn produce_messages(self, output: impl Recipient<M>) -> Result<(), RuntimeError>;
}

/// Akin to `/dev/null`
///
/// - Produce no messages
/// - Consume any message, silently dropping them
#[derive(Clone, Debug)]
pub struct DevNull;

#[async_trait]
impl<M: Message> Producer<M> for DevNull {
    async fn produce_messages(self, _output: impl Recipient<M>) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[async_trait]
impl<M: Message> Recipient<M> for DevNull {
    async fn send_message(&mut self, _message: M) -> Result<(), RuntimeError> {
        Ok(())
    }
}

/// Strings can be used as Message
impl Message for String {}

/// An actor can have no input or no output messages
#[derive(Clone, Debug)]
pub enum NoMessage {}
impl Message for NoMessage {}
