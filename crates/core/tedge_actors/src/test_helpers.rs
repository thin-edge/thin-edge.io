//! Testing actors
use crate::Builder;
use crate::ChannelError;
use crate::DynSender;
use crate::Message;
use crate::MessageReceiver;
use crate::MessageSink;
use crate::NoMessage;
use crate::RequestEnvelope;
use crate::RuntimeRequest;
use crate::Sender;
use crate::SimpleMessageBox;
use crate::SimpleMessageBoxBuilder;
use async_trait::async_trait;
use core::future::Future;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::fmt::Debug;
use std::time::Duration;
use tokio::time::timeout;
use tokio::time::Timeout;

/// A test helper that extends a message box with various way to check received messages.
#[async_trait]
pub trait MessageReceiverExt<M: Message>: Sized {
    /// Return a new receiver which returns None if no message is received after the given timeout
    ///
    /// ```
    /// # use tedge_actors::{Builder, NoConfig, NoMessage, MessageReceiver, RuntimeError, Sender, SimpleMessageBox, SimpleMessageBoxBuilder};
    /// # use std::time::Duration;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),RuntimeError> {
    ///
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(NoConfig, &mut receiver_builder);
    /// let mut sender = sender_builder.build();
    /// let receiver: SimpleMessageBox<&str,NoMessage> = receiver_builder.build();
    ///
    /// use tedge_actors::test_helpers::MessageReceiverExt;
    /// let mut receiver = receiver.with_timeout(Duration::from_millis(100));
    ///
    /// sender.send("Hello").await?;
    /// sender.send("World").await?;
    ///
    /// assert_eq!(receiver.recv().await, Some("Hello"));
    /// assert_eq!(receiver.recv().await, Some("World"));
    /// assert_eq!(receiver.recv().await, None);
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Note that, calling `MessageReceiverExt.with_timeout()` on a receiver returns an `impl MessageReceiver`
    /// discarding any other traits implemented by the former receiver.
    /// You will have to use `as_ref()` or `as_mut()` to access the wrapped message box.
    ///
    /// ```
    /// # use crate::tedge_actors::{Builder, RuntimeError, Sender, SimpleMessageBox, SimpleMessageBoxBuilder};
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),RuntimeError> {
    ///
    /// use std::time::Duration;
    /// use tedge_actors::test_helpers::MessageReceiverExt;
    /// let message_box: SimpleMessageBox<&str,&str> = SimpleMessageBoxBuilder::new("Box",16).build();
    ///
    /// // The timeout_receiver is no more a message_box
    /// let mut timeout_receiver = message_box.with_timeout(Duration::from_millis(100));
    ///
    /// // However the inner message_box can still be accessed
    /// timeout_receiver.send("Hello world").await?;
    ///
    /// # Ok(())
    /// }
    /// ```
    ///
    fn with_timeout(self, timeout: Duration) -> TimedMessageBox<Self>;

    /// Skip the given number of messages
    ///
    /// ```
    /// # use tedge_actors::{Builder, NoConfig, NoMessage, MessageReceiver, RuntimeError, Sender, SimpleMessageBox, SimpleMessageBoxBuilder};
    /// # use std::time::Duration;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),RuntimeError> {
    ///
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(NoConfig, &mut receiver_builder);
    /// let mut sender = sender_builder.build();
    /// let mut receiver: SimpleMessageBox<&str,NoMessage> = receiver_builder.build();
    ///
    /// sender.send("Boring message").await?;
    /// sender.send("Boring message").await?;
    /// sender.send("Hello World").await?;
    ///
    /// use tedge_actors::test_helpers::MessageReceiverExt;
    /// receiver.skip(2).await;
    /// assert_eq!(receiver.recv().await, Some("Hello World"));
    ///
    /// # Ok(())
    /// # }
    /// ```
    async fn skip(&mut self, count: usize);

    /// Check that all messages are received in the given order without any interleaved messages.
    ///
    /// ```rust
    /// # use crate::tedge_actors::{Builder, NoConfig, NoMessage, RuntimeError, Sender, SimpleMessageBox, SimpleMessageBoxBuilder, test_helpers};
    /// # use std::time::Duration;
    /// #[derive(Debug,Eq,PartialEq)]
    /// enum MyMessage {
    ///    Foo(u32),
    ///    Bar(u32),
    /// }
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),RuntimeError> {
    ///
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(NoConfig, &mut receiver_builder);
    /// let mut sender = sender_builder.build();
    /// let receiver: SimpleMessageBox<MyMessage,NoMessage> = receiver_builder.build();
    ///
    /// use tedge_actors::test_helpers::MessageReceiverExt;
    /// let mut receiver = receiver.with_timeout(Duration::from_millis(100));
    ///
    /// sender.send(MyMessage::Foo(1)).await?;
    /// sender.send(MyMessage::Bar(2)).await?;
    /// sender.send(MyMessage::Foo(3)).await?;
    ///
    /// receiver.assert_received([
    ///     MyMessage::Foo(1),
    ///     MyMessage::Bar(2),
    ///     MyMessage::Foo(3),
    /// ]).await;
    ///
    /// # Ok(())
    /// # }
    ///
    /// ```
    async fn assert_received<Samples>(&mut self, expected: Samples)
    where
        Samples: IntoIterator + Send,
        M: From<Samples::Item>;

    /// Check that all messages are received possibly in a different order or with interleaved messages.
    ///
    /// ```rust
    /// use crate::tedge_actors::{Builder, NoMessage, RuntimeError, Sender, SimpleMessageBox, SimpleMessageBoxBuilder, test_helpers};
    ///
    /// #[derive(Debug,Eq,PartialEq)]
    /// enum MyMessage {
    ///    Foo(u32),
    ///    Bar(u32),
    /// }
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),RuntimeError> {
    ///
    /// # use std::time::Duration;
    /// # use tedge_actors::NoConfig;
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(NoConfig, &mut receiver_builder);
    /// let mut sender = sender_builder.build();
    /// let receiver: SimpleMessageBox<MyMessage,NoMessage> = receiver_builder.build();
    ///
    /// use tedge_actors::test_helpers::MessageReceiverExt;
    /// let mut receiver = receiver.with_timeout(Duration::from_millis(100));
    ///
    /// sender.send(MyMessage::Foo(1)).await?;
    /// sender.send(MyMessage::Bar(2)).await?;
    /// sender.send(MyMessage::Foo(3)).await?;
    ///
    /// receiver.assert_received_unordered([
    ///     MyMessage::Foo(3),
    ///     MyMessage::Bar(2),
    /// ]).await;
    ///
    /// # Ok(())
    /// # }
    ///
    /// ```
    async fn assert_received_unordered<Samples>(&mut self, expected: Samples)
    where
        Samples: IntoIterator + Send,
        M: From<Samples::Item>;

    /// Check that at least one matching message is received for each pattern.
    ///
    /// The messages can possibly be received in a different order or with interleaved messages.
    ///
    /// ```rust
    /// use crate::tedge_actors::{Builder, NoMessage, RuntimeError, Sender, SimpleMessageBox, SimpleMessageBoxBuilder, test_helpers};
    ///
    /// #[derive(Debug,Eq,PartialEq)]
    /// enum MyMessage {
    ///    Foo(u32),
    ///    Bar(u32),
    /// }
    ///
    /// impl MyMessage {
    ///     pub fn count(&self) -> u32 {
    ///         match self {
    ///             MyMessage::Foo(n) => *n,
    ///             MyMessage::Bar(n) => *n,
    ///         }
    ///     }
    /// }
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),RuntimeError> {
    ///
    /// # use std::time::Duration;
    /// # use tedge_actors::NoConfig;
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(NoConfig, &mut receiver_builder);
    /// let mut sender = sender_builder.build();
    /// let receiver: SimpleMessageBox<MyMessage,NoMessage> = receiver_builder.build();
    ///
    /// use tedge_actors::test_helpers::MessageReceiverExt;
    /// let mut receiver = receiver.with_timeout(Duration::from_millis(100));
    ///
    /// sender.send(MyMessage::Foo(1)).await?;
    /// sender.send(MyMessage::Bar(2)).await?;
    /// sender.send(MyMessage::Foo(3)).await?;
    ///
    /// receiver.assert_received_matching(
    ///     |pat:&u32,msg:&MyMessage| msg.count() == *pat,
    ///     [3,2],
    /// ).await;
    ///
    /// # Ok(())
    /// # }
    ///
    /// ```
    async fn assert_received_matching<T, F>(&mut self, matching: F, expected: T)
    where
        T: IntoIterator + Send,
        F: Fn(&T::Item, &M) -> bool,
        F: Send,
        T::Item: Debug + Send;
}

#[async_trait]
impl<T, M> MessageReceiverExt<M> for T
where
    T: MessageReceiver<M> + Send + Sync + 'static,
    M: Message + Eq + PartialEq,
{
    fn with_timeout(self, timeout: Duration) -> TimedMessageBox<Self> {
        TimedMessageBox {
            timeout,
            inner: self,
        }
    }

    async fn skip(&mut self, count: usize) {
        for _ in 0..count {
            assert!(self.recv().await.is_some());
        }
    }

    #[allow(clippy::needless_collect)] // To avoid issues with Send constraints
    async fn assert_received<Samples>(&mut self, expected: Samples)
    where
        Samples: IntoIterator + Send,
        M: From<Samples::Item>,
    {
        let expected: Vec<M> = expected.into_iter().map(|msg| msg.into()).collect();
        for expected_msg in expected.into_iter() {
            let actual_msg = self.recv().await;
            assert_eq!(actual_msg, Some(expected_msg));
        }
    }

    async fn assert_received_unordered<Samples>(&mut self, expected: Samples)
    where
        Samples: IntoIterator + Send,
        M: From<Samples::Item>,
    {
        let expected: Vec<M> = expected.into_iter().map(|msg| msg.into()).collect();
        self.assert_received_matching(|pat: &M, msg: &M| pat == msg, expected)
            .await
    }

    async fn assert_received_matching<Samples, F>(&mut self, matching: F, expected: Samples)
    where
        Samples: IntoIterator + Send,
        F: Fn(&Samples::Item, &M) -> bool,
        F: Send,
        Samples::Item: Debug + Send,
    {
        let mut expected: Vec<Samples::Item> = expected.into_iter().collect();
        let mut received = Vec::new();

        while let Some(msg) = self.recv().await {
            expected.retain(|pat| !matching(pat, &msg));
            received.push(msg);
            if expected.is_empty() {
                return;
            }
        }

        assert!(
            expected.is_empty(),
            "Didn't receive all expected messages:\n\tMissing a match for: {expected:?}\n\tReceived: {received:?}",
        );
    }
}

/// A message box that behaves as if the channel has been closed on recv,
/// returning None, when no message is received after a given duration.
pub struct TimedMessageBox<T> {
    timeout: Duration,
    inner: T,
}

impl<T: Clone> Clone for TimedMessageBox<T> {
    fn clone(&self) -> Self {
        TimedMessageBox {
            timeout: self.timeout,
            inner: self.inner.clone(),
        }
    }
}

#[async_trait]
impl<T, M> MessageReceiver<M> for TimedMessageBox<T>
where
    M: Message,
    T: MessageReceiver<M> + Send + Sync + 'static,
{
    async fn try_recv(&mut self) -> Result<Option<M>, RuntimeRequest> {
        tokio::time::timeout(self.timeout, self.inner.try_recv())
            .await
            .unwrap_or(Ok(None))
    }

    async fn recv(&mut self) -> Option<M> {
        tokio::time::timeout(self.timeout, self.inner.recv())
            .await
            .unwrap_or(None)
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        tokio::time::timeout(self.timeout, self.inner.recv_signal())
            .await
            .unwrap_or(None)
    }
}

#[async_trait]
impl<T, M> Sender<M> for TimedMessageBox<T>
where
    M: Message,
    T: Sender<M>,
{
    async fn send(&mut self, message: M) -> Result<(), ChannelError> {
        self.inner.send(message).await
    }
}

impl<T> AsRef<T> for TimedMessageBox<T> {
    fn as_ref(&self) -> &T {
        &self.inner
    }
}

impl<T> AsMut<T> for TimedMessageBox<T> {
    fn as_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

pub trait WithTimeout<T>
where
    T: Future,
{
    fn with_timeout(self, duration: Duration) -> Timeout<T>;
}

impl<F> WithTimeout<F> for F
where
    F: Future,
{
    fn with_timeout(self, duration: Duration) -> Timeout<F> {
        timeout(duration, self)
    }
}

/// A message box to mimic the behavior of an actor server.
///
/// This fake server panics on error.
pub struct FakeServerBox<Request: Debug, Response> {
    /// The received messages are the requests sent by the client under test
    /// and the published messages are the responses given by the test driver.
    messages: SimpleMessageBox<RequestEnvelope<Request, Response>, NoMessage>,

    /// Where to send the response for the current request, if any
    reply_to: VecDeque<Box<dyn Sender<Response>>>,
}

impl<Request: Message, Response: Message> FakeServerBox<Request, Response> {
    /// Return a fake message box builder
    pub fn builder() -> FakeServerBoxBuilder<Request, Response> {
        FakeServerBoxBuilder::default()
    }
}

#[async_trait]
impl<Request: Message, Response: Message> MessageReceiver<Request>
    for FakeServerBox<Request, Response>
{
    async fn try_recv(&mut self) -> Result<Option<Request>, RuntimeRequest> {
        match self.messages.try_recv().await {
            Ok(None) => Ok(None),
            Ok(Some(RequestEnvelope { request, reply_to })) => {
                self.reply_to.push_back(reply_to);
                Ok(Some(request))
            }
            Err(signal) => Err(signal),
        }
    }

    async fn recv(&mut self) -> Option<Request> {
        match self.messages.recv().await {
            None => None,
            Some(RequestEnvelope { request, reply_to }) => {
                self.reply_to.push_back(reply_to);
                Some(request)
            }
        }
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.messages.recv_signal().await
    }
}

#[async_trait]
impl<Request: Message, Response: Message> Sender<Response> for FakeServerBox<Request, Response> {
    async fn send(&mut self, response: Response) -> Result<(), ChannelError> {
        let mut reply_to = self
            .reply_to
            .pop_front()
            .expect("Nobody is expecting a response");
        reply_to.send(response).await
    }
}

pub struct FakeServerBoxBuilder<Request: Debug, Response> {
    messages: SimpleMessageBoxBuilder<RequestEnvelope<Request, Response>, NoMessage>,
}

impl<Request: Message, Response: Message> Default for FakeServerBoxBuilder<Request, Response> {
    fn default() -> Self {
        FakeServerBoxBuilder {
            messages: SimpleMessageBoxBuilder::new("Fake Server", 16),
        }
    }
}

impl<Request: Message, Response: Message> MessageSink<RequestEnvelope<Request, Response>>
    for FakeServerBoxBuilder<Request, Response>
{
    fn get_sender(&self) -> DynSender<RequestEnvelope<Request, Response>> {
        self.messages.get_sender()
    }
}

impl<Request: Message, Response: Message> Builder<FakeServerBox<Request, Response>>
    for FakeServerBoxBuilder<Request, Response>
{
    type Error = Infallible;

    fn try_build(self) -> Result<FakeServerBox<Request, Response>, Infallible> {
        Ok(FakeServerBox {
            messages: self.messages.build(),
            reply_to: VecDeque::new(),
        })
    }
}
