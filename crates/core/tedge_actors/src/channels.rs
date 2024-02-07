//! Sending and receiving messages
use crate::ChannelError;
use crate::Message;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::SinkExt;

/// A sender of messages of type `M`
///
/// Actors don't get direct access to the `mpsc::Sender` of their peers,
/// but use intermediate senders that adapt the messages when sent.
pub type DynSender<M> = Box<dyn CloneSender<M>>;

#[async_trait]
pub trait Sender<M>: 'static + Send + Sync {
    /// Send a message to the receiver behind this sender,
    /// returning an error if the receiver is no more expecting messages
    async fn send(&mut self, message: M) -> Result<(), ChannelError>;
}

pub trait CloneSender<M>: Sender<M> {
    /// Clone this sender in order to send messages to the same receiver from another actor
    fn sender_clone(&self) -> DynSender<M>;

    /// Clone a cast of this sender into a `Box<dyn Sender<M>>`
    ///
    /// This is a workaround for https://github.com/rust-lang/rust/issues/65991
    fn sender(&self) -> Box<dyn Sender<M>>;
}

impl<M, S: Clone + Sender<M>> CloneSender<M> for S {
    fn sender_clone(&self) -> DynSender<M> {
        Box::new(self.clone())
    }

    fn sender(&self) -> Box<dyn Sender<M>> {
        Box::new(self.clone())
    }
}

impl<M, S: Clone + Sender<M>> From<S> for DynSender<M> {
    fn from(sender: S) -> Self {
        Box::new(sender)
    }
}

/// A `DynSender<M>` is a `DynSender<N>` provided `N` implements `Into<M>`
#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for DynSender<M> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        Ok(self.as_mut().send(message.into()).await?)
    }
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for Box<dyn Sender<M>> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        Ok(self.as_mut().send(message.into()).await?)
    }
}

#[async_trait]
impl<M: Message, N: Message + Into<M>> CloneSender<N> for DynSender<M> {
    fn sender_clone(&self) -> DynSender<N> {
        Box::new(self.as_ref().sender_clone())
    }

    fn sender(&self) -> Box<dyn Sender<N>> {
        Box::new(self.as_ref().sender())
    }
}

/// An `mpsc::Sender<M>` is a `DynSender<M>`
#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for mpsc::Sender<M> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        Ok(SinkExt::send(&mut self, message.into()).await?)
    }
}

/// An `mpsc::UnboundedSender<M>` is a `DynSender<N>` provided `N` implements `Into<M>`
#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for mpsc::UnboundedSender<M> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        Ok(SinkExt::send(&mut self, message.into()).await?)
    }
}

/// A `oneshot::Sender<M>` is a `Sender<N>` provided `N` implements `Into<M>`
///
/// There is one caveat. The `oneshot::Sender::send()` method consumes the sender,
/// hence the one shot sender is wrapped inside an `Option`.
///
/// Such a [Sender] can only be used once:
/// - it cannot be cloned
/// - any message sent after a first one will be silently ignored
/// - a message sent while the receiver has been drop will also be silently ignored
#[async_trait]
impl<M: Message, N: Message + Into<M>> Sender<N> for Option<oneshot::Sender<M>> {
    async fn send(&mut self, message: N) -> Result<(), ChannelError> {
        if let Some(sender) = self.take() {
            let _ = sender.send(message.into());
        }
        Ok(())
    }
}

/// A sender that discards messages instead of sending them
#[derive(Clone)]
pub struct NullSender;

#[async_trait]
impl<M: Message> Sender<M> for NullSender {
    async fn send(&mut self, _message: M) -> Result<(), ChannelError> {
        Ok(())
    }
}

/// A sender that transforms the messages on the fly
pub struct MappingSender<F, M> {
    inner: DynSender<M>,
    cast: std::sync::Arc<F>,
}

impl<F, M: 'static> Clone for MappingSender<F, M> {
    fn clone(&self) -> Self {
        MappingSender {
            inner: self.inner.sender_clone(),
            cast: self.cast.clone(),
        }
    }
}

impl<F, M> MappingSender<F, M> {
    pub fn new(inner: DynSender<M>, cast: F) -> Self {
        MappingSender {
            inner,
            cast: std::sync::Arc::new(cast),
        }
    }
}

#[async_trait]
impl<M, N, NS, F> Sender<M> for MappingSender<F, N>
where
    M: Message,
    N: Message,
    NS: Iterator<Item = N> + Send,
    F: Fn(M) -> NS,
    F: 'static + Sync + Send,
{
    async fn send(&mut self, message: M) -> Result<(), ChannelError> {
        for out_message in self.cast.as_ref()(message) {
            self.inner.send(out_message).await?
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fan_in_message_type;
    use futures::StreamExt;

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
            receiver.collect::<Vec<_>>().await,
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
                peer_1: recipient.sender_clone(),
                peer_2: recipient.sender_clone(),
            }
        }
    }

    #[tokio::test]
    async fn a_recipient_can_be_adapted_to_accept_sub_messages_from_several_sources() {
        let (sender, mut receiver) = mpsc::channel(10);

        {
            let dyn_sender: DynSender<Msg> = sender.into();
            let mut peers = Peers::from(dyn_sender);
            peers.peer_1.send(Msg1 {}).await.unwrap();
            peers.peer_2.send(Msg2 {}).await.unwrap();

            // the sender is drop here => the receiver will receive a None for end of stream.
        }

        assert_eq!(receiver.next().await, Some(Msg::Msg1(Msg1 {})));
        assert_eq!(receiver.next().await, Some(Msg::Msg2(Msg2 {})));
        assert_eq!(receiver.next().await, None);
    }
}
