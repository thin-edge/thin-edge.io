use crate::{Message, RuntimeError};
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::fmt::{Debug, Formatter};

/// A recipient for messages of type `M`
pub type Recipient<M> = Box<dyn Sender<M>>;

impl<M: 'static> Debug for Recipient<M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[async_trait]
pub trait Sender<M>: 'static + Send + Sync {
    /// Name of the recipient
    fn name(&self) -> &str {
        "anonymous recipient"
    }

    /// Send a message returning an error if the recipient is no more expecting messages
    async fn send_message(&mut self, message: M) -> Result<(), RuntimeError>;

    /// Clone this handle in order to send messages to the same recipient from another thread
    fn clone(&self) -> Recipient<M>;
}

/// An address where messages of type `Into<M>` can be sent
#[derive(Clone, Debug)]
pub struct Address<M> {
    sender: mpsc::UnboundedSender<M>,
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for Address<M> {
    async fn send_message(&mut self, message: N) -> Result<(), RuntimeError> {
        Ok(self.sender.send(message.into()).await?)
    }

    fn clone(&self) -> Box<dyn Sender<N>> {
        Box::new(Clone::clone(self))
    }
}

impl<M: Message> Into<Recipient<M>> for Address<M> {
    fn into(self) -> Recipient<M> {
        Box::new(self)
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
