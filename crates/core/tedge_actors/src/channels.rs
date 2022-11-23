use crate::{ChannelError, Message};
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::fmt::{Debug, Formatter};

/// Create a new mailbox with its address
///
/// Such a mailbox is used by an actor to receive all its messages.
/// Clones of the address are given to the sending peers.
pub fn new_mailbox<M>(bound: usize) -> (Mailbox<M>, Address<M>) {
    let (sender, receiver) = mpsc::channel(bound);
    let address = Address { sender };
    let mailbox = Mailbox { receiver };
    (mailbox, address)
}

/// A mailbox that gather *all* the messages sent to an actor
pub struct Mailbox<M> {
    receiver: mpsc::Receiver<M>,
}

impl<M> Mailbox<M> {
    /// Pop from the mailbox the message with the highest priority
    ///
    /// Await till a messages is available.
    /// Return `None` when all the senders to this mailbox have been dropped and all the messages consumed.
    pub async fn next(&mut self) -> Option<M> {
        self.receiver.next().await
    }

    /// Collect all the messages of the mailbox into a vector
    ///
    /// Mostly useful for testing.
    /// Note that this will block until there is no more senders,
    /// .i.e. the mailbox address and all its clones have been dropped.
    pub async fn collect(mut self) -> Vec<M> {
        let mut messages = vec![];
        while let Some(message) = self.next().await {
            messages.push(message);
        }
        messages
    }
}

/// The address of an actor
pub struct Address<M> {
    sender: mpsc::Sender<M>,
}

// The derive macro incorrectly requires M to be Clone
impl<M> Clone for Address<M> {
    fn clone(&self) -> Self {
        Address {
            sender: self.sender.clone(),
        }
    }
}

impl<M: Message> Address<M> {
    /// Build a clone of this address to used as a recipient of sub-messages,
    /// i.e. messages that can be cast into those expected by the mailbox.
    pub fn as_recipient<N: Message + Into<M>>(&self) -> Recipient<N> {
        self.clone().into()
    }
}

/// A recipient for messages of type `M`
///
/// Actors don't access directly the addresses of their peers,
/// but use intermediate recipients that adapt the messages when sent.
pub type Recipient<M> = Box<dyn Sender<M>>;

#[async_trait]
pub trait Sender<M>: 'static + Send + Sync {
    /// Send a message to the recipient,
    /// returning an error if the recipient is no more expecting messages
    async fn send(&mut self, message: M) -> Result<(), ChannelError>;

    /// Clone this sender in order to send messages to the same recipient from another actor
    fn recipient_clone(&self) -> Recipient<M>;
}

/// An `Address<M>` is a `Recipient<N>` provided `N` implements `Into<M>`
impl<M: Message, N: Message + Into<M>> From<Address<M>> for Recipient<N> {
    fn from(address: Address<M>) -> Self {
        Box::new(address)
    }
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for Address<M> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        Ok(self.sender.send(message.into()).await?)
    }

    fn recipient_clone(&self) -> Recipient<N> {
        Box::new(self.clone())
    }
}

impl<M: Message> Debug for Recipient<M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("anonymous recipient")
    }
}

impl<M: Message> Clone for Recipient<M> {
    fn clone(&self) -> Self {
        self.recipient_clone()
    }
}

/// Make a `Recipient<N>` from a `Recipient<M>`
///
/// This is a workaround to the fact the compiler rejects a From implementation:
///
/// ```shell
///
///  impl<M: Message, N: Message + Into<M>> From<Recipient<M>> for Recipient<N> {
///     | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
///     |
///     = note: conflicting implementation in crate `core`:
///             - impl<T> From<T> for T;
/// ```
pub fn adapt<M: Message, N: Message + Into<M>>(sender: &Recipient<M>) -> Recipient<N> {
    Box::new(Adapter {
        sender: sender.recipient_clone(),
    })
}

struct Adapter<M> {
    sender: Recipient<M>,
}

impl<M: Message, N: Message + Into<M>> From<Adapter<M>> for Recipient<N> {
    fn from(adapter: Adapter<M>) -> Self {
        Box::new(adapter)
    }
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for Adapter<M> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        Ok(self.sender.send(message.into()).await?)
    }

    fn recipient_clone(&self) -> Recipient<N> {
        Box::new(Adapter {
            sender: self.sender.recipient_clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fan_in_message_type;
    use crate::test_utils::VecRecipient;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Msg1 {}

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Msg2 {}

    fan_in_message_type!(Msg[Msg1,Msg2] : Clone , Debug , Eq , PartialEq);

    #[tokio::test]
    async fn an_address_is_a_recipient_of_sub_msg() {
        let (mailbox, address) = new_mailbox::<Msg>(10);

        {
            let mut address = address;
            let mut recipient_msg1: Recipient<Msg1> = address.as_recipient();
            let mut recipient_msg2 = address.as_recipient();

            address
                .send(Msg::Msg1(Msg1 {}))
                .await
                .expect("enough room in the mailbox");
            recipient_msg1
                .send(Msg1 {})
                .await
                .expect("enough room in the mailbox");
            recipient_msg2
                .send(Msg2 {})
                .await
                .expect("enough room in the mailbox");
        }

        assert_eq!(
            mailbox.collect().await,
            vec![Msg::Msg1(Msg1 {}), Msg::Msg1(Msg1 {}), Msg::Msg2(Msg2 {}),]
        )
    }

    pub struct Peers {
        pub peer_1: Recipient<Msg1>,
        pub peer_2: Recipient<Msg2>,
    }

    impl From<Recipient<Msg>> for Peers {
        fn from(recipient: Recipient<Msg>) -> Self {
            Peers {
                peer_1: adapt(&recipient),
                peer_2: adapt(&recipient),
            }
        }
    }

    #[tokio::test]
    async fn a_recipient_can_be_adapted_to_accept_sub_messages_from_several_sources() {
        let messages: VecRecipient<Msg> = VecRecipient::default();
        let recipient = messages.as_recipient();

        let mut peers = Peers::from(recipient);
        peers.peer_1.send(Msg1 {}).await.unwrap();
        peers.peer_2.send(Msg2 {}).await.unwrap();

        assert_eq!(
            messages.collect().await,
            vec![Msg::Msg1(Msg1 {}), Msg::Msg2(Msg2 {}),]
        )
    }
}
