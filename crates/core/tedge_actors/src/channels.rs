use crate::{ChannelError, Message};
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::fmt::{Debug, Formatter};

pub type Address<M> = mpsc::Sender<M>;

/// Create a new mailbox with its address
///
/// Such a mailbox is used by an actor to receive all its messages.
/// Clones of the address are given to the sending peers.
pub fn new_mailbox<M>(bound: usize) -> (Mailbox<M>, Address<M>) {
    let (sender, receiver) = mpsc::channel(bound);
    let mailbox = Mailbox { receiver };
    (mailbox, sender)
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

/// A sender of messages of type `M`
///
/// Actors don't access directly the addresses of their peers,
/// but use intermediate senders that adapt the messages when sent.
pub type DynSender<M> = Box<dyn Sender<M>>;

#[async_trait]
pub trait Sender<M>: 'static + Send + Sync {
    /// Send a message to the receiver behind this sender,
    /// returning an error if the receiver is no more expecting messages
    async fn send(&mut self, message: M) -> Result<(), ChannelError>;

    /// Clone this sender in order to send messages to the same receiver from another actor
    fn sender_clone(&self) -> DynSender<M>;
}

/// An `mpsc::Sender<M>` is a `DynSender<N>` provided `N` implements `Into<M>`
impl<M: Message, N: Message + Into<M>> From<mpsc::Sender<M>> for DynSender<N> {
    fn from(address: mpsc::Sender<M>) -> Self {
        Box::new(address)
    }
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for mpsc::Sender<M> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        Ok(SinkExt::send(&mut self, message.into()).await?)
    }

    fn sender_clone(&self) -> DynSender<N> {
        Box::new(self.clone())
    }
}

impl<M: Message> Debug for DynSender<M> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("anonymous sender")
    }
}

impl<M: Message> Clone for DynSender<M> {
    fn clone(&self) -> Self {
        self.sender_clone()
    }
}

/// Make a `DynSender<N>` from a `DynSender<M>`
///
/// This is a workaround to the fact the compiler rejects a From implementation:
///
/// ```shell
///
///  impl<M: Message, N: Message + Into<M>> From<DynSender<M>> for DynSender<N> {
///     | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
///     |
///     = note: conflicting implementation in crate `core`:
///             - impl<T> From<T> for T;
/// ```
pub fn adapt<M: Message, N: Message + Into<M>>(sender: &DynSender<M>) -> DynSender<N> {
    Box::new(Adapter {
        sender: sender.sender_clone(),
    })
}

struct Adapter<M> {
    sender: DynSender<M>,
}

impl<M: Message, N: Message + Into<M>> From<Adapter<M>> for DynSender<N> {
    fn from(adapter: Adapter<M>) -> Self {
        Box::new(adapter)
    }
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for Adapter<M> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        Ok(self.sender.send(message.into()).await?)
    }

    fn sender_clone(&self) -> DynSender<N> {
        Box::new(Adapter {
            sender: self.sender.sender_clone(),
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
            let mut recipient_msg1: DynSender<Msg1> = address.clone().into();
            let mut recipient_msg2: DynSender<Msg2> = address.clone().into();

            SinkExt::send(&mut address, Msg::Msg1(Msg1 {}))
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
        pub peer_1: DynSender<Msg1>,
        pub peer_2: DynSender<Msg2>,
    }

    impl From<DynSender<Msg>> for Peers {
        fn from(recipient: DynSender<Msg>) -> Self {
            Peers {
                peer_1: adapt(&recipient),
                peer_2: adapt(&recipient),
            }
        }
    }

    #[tokio::test]
    async fn a_recipient_can_be_adapted_to_accept_sub_messages_from_several_sources() {
        let messages: VecRecipient<Msg> = VecRecipient::default();
        let recipient = messages.as_sender();

        let mut peers = Peers::from(recipient);
        peers.peer_1.send(Msg1 {}).await.unwrap();
        peers.peer_2.send(Msg2 {}).await.unwrap();

        assert_eq!(
            messages.collect().await,
            vec![Msg::Msg1(Msg1 {}), Msg::Msg2(Msg2 {}),]
        )
    }
}
