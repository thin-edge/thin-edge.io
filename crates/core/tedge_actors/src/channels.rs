use crate::{ChannelError, Message};
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::SinkExt;
use std::fmt::{Debug, Formatter};

/// A sender of messages of type `M`
///
/// Actors don't access directly the `mpsc::Sender` of their peers,
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
    fn from(sender: mpsc::Sender<M>) -> Self {
        Box::new(sender)
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
    use crate::test_utils::collect;
    use crate::test_utils::VecRecipient;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Msg1 {}

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Msg2 {}

    fan_in_message_type!(Msg[Msg1,Msg2] : Clone , Debug , Eq , PartialEq);

    #[tokio::test]
    async fn an_mpsc_sender_is_a_recipient_of_sub_msg() {
        let (sender, receiver) = mpsc::channel::<Msg>(10);

        {
            let mut sender = sender;
            let mut sender_msg1: DynSender<Msg1> = sender.clone().into();
            let mut sender_msg2: DynSender<Msg2> = sender.clone().into();

            SinkExt::send(&mut sender, Msg::Msg1(Msg1 {}))
                .await
                .expect("enough room in the receiver queue");
            sender_msg1
                .send(Msg1 {})
                .await
                .expect("enough room in the receiver queue");
            sender_msg2
                .send(Msg2 {})
                .await
                .expect("enough room in the receiver queue");
        }

        assert_eq!(
            collect(receiver).await,
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
