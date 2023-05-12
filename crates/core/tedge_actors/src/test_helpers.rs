//! Testing actors
use crate::mpsc;
use crate::Builder;
use crate::ChannelError;
use crate::DynSender;
use crate::Message;
use crate::MessageReceiver;
use crate::MessageSink;
use crate::MessageSource;
use crate::NoConfig;
use crate::NullSender;
use crate::RuntimeRequest;
use crate::Sender;
use crate::ServiceConsumer;
use crate::ServiceProvider;
use crate::SimpleMessageBox;
use crate::SimpleMessageBoxBuilder;
use crate::WrappedInput;
use async_trait::async_trait;
use core::future::Future;
use futures::stream::FusedStream;
use futures::SinkExt;
use futures::StreamExt;
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
    /// # use tedge_actors::{Builder, NoMessage, MessageReceiver, RuntimeError, Sender, ServiceConsumer, SimpleMessageBox, SimpleMessageBoxBuilder};
    /// # use std::time::Duration;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),RuntimeError> {
    ///
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(&mut receiver_builder);
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
    /// # use tedge_actors::{Builder, NoMessage, MessageReceiver, RuntimeError, Sender, ServiceConsumer, SimpleMessageBox, SimpleMessageBoxBuilder};
    /// # use std::time::Duration;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),RuntimeError> {
    ///
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(&mut receiver_builder);
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
    /// # use crate::tedge_actors::{Builder, NoMessage, RuntimeError, Sender, ServiceConsumer, SimpleMessageBox, SimpleMessageBoxBuilder, test_helpers};
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
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(&mut receiver_builder);
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
    /// use crate::tedge_actors::{Builder, NoMessage, RuntimeError, Sender, ServiceConsumer, SimpleMessageBox, SimpleMessageBoxBuilder, test_helpers};
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
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(&mut receiver_builder);
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
    /// use crate::tedge_actors::{Builder, NoMessage, RuntimeError, Sender, ServiceConsumer, SimpleMessageBox, SimpleMessageBoxBuilder, test_helpers};
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
    /// use std::time::Duration;
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let sender_builder = SimpleMessageBoxBuilder::new("Send", 16).with_connection(&mut receiver_builder);
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
            let _ = self.recv().await;
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

    async fn recv_message(&mut self) -> Option<WrappedInput<M>> {
        tokio::time::timeout(self.timeout, self.inner.recv_message())
            .await
            .unwrap_or(None)
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

    fn sender_clone(&self) -> DynSender<M> {
        self.inner.sender_clone()
    }

    fn close_sender(&mut self) {
        self.inner.close_sender()
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

/// A message that can be broadcast
pub trait MessagePlus: Message + Clone + Eq {}
impl<T: Message + Clone + Eq> MessagePlus for T {}

/// For testing purpose, a `Probe` can be interposed between two actors to observe their interactions.
///
/// The two actors under test, as well as their message boxes, are built and launched as usual,
/// the only interaction being on the wire:
/// - A probe is set on one side using the [with_probe()](crate::test_helpers::ServiceConsumerExt::with_probe)
///   method added by the [ServiceConsumerExt](crate::test_helpers::ServiceConsumerExt)
///   to any actor or message box builder.
/// - The [Probe::observe()](crate::test_helpers::Probe::observe) method can then be used
///   to observe all the messages either [sent](crate::test_helpers::ProbeEvent::Send)
///   or [received](crate::test_helpers::ProbeEvent::Recv) by the actor on which the probe has been set.
///
/// ```
/// # use tedge_actors::{Actor, Builder, ChannelError, ServiceConsumer, NoConfig, ServerActor, ServerMessageBoxBuilder, SimpleMessageBoxBuilder};
///
/// # use tedge_actors::test_helpers::ProbeEvent::{Recv, Send};
/// # use crate::tedge_actors::examples::calculator_server::*;
/// # #[tokio::main]
/// # async fn main_test() -> Result<(),ChannelError> {
/// #
/// // The [ServiceConsumerExt] trait must be in scope to add the `with_probe()` method on actor builders
/// use tedge_actors::test_helpers::{ServiceConsumerExt, Probe, ProbeEvent};
///
/// // Build the actor message boxes
/// let mut server_box_builder = ServerMessageBoxBuilder::new("Calculator", 16);
/// let mut player_box_builder = SimpleMessageBoxBuilder::new("Player 1", 1);
///
/// // Connect the two actor message boxes interposing a probe.
/// let mut probe = Probe::new();
/// player_box_builder.with_probe(&mut probe).set_connection(&mut server_box_builder);
///
/// // Spawn the actors
/// let calculator = Calculator::default();
/// tokio::spawn(async move { ServerActor::new(calculator, server_box_builder.build()).run().await } );
/// tokio::spawn(async move { Player { name: "Player".to_string(), target: 42, messages: player_box_builder.build()}.run().await } );
///
/// // Observe the messages sent and received by the player.
/// assert_eq!(probe.observe().await, Send(Operation::Add(0)));
/// assert_eq!(probe.observe().await, Recv(Update{from:0, to:0}));
/// assert_eq!(probe.observe().await, Send(Operation::Add(21)));
/// assert_eq!(probe.observe().await, Recv(Update{from:0, to:21}));
/// assert_eq!(probe.observe().await, Send(Operation::Add(10)));
/// assert_eq!(probe.observe().await, Recv(Update{from:21, to:31}));
/// assert_eq!(probe.observe().await, Send(Operation::Add(5)));
/// assert_eq!(probe.observe().await, Recv(Update{from:31, to:36}));
/// assert_eq!(probe.observe().await, Send(Operation::Add(3)));
/// assert_eq!(probe.observe().await, Recv(Update{from:36, to:39}));
/// assert_eq!(probe.observe().await, Send(Operation::Add(1)));
/// assert_eq!(probe.observe().await, Recv(Update{from:39, to:40}));
/// assert_eq!(probe.observe().await, Send(Operation::Add(1)));
/// assert_eq!(probe.observe().await, Recv(Update{from:40, to:41}));
/// assert_eq!(probe.observe().await, Send(Operation::Add(0)));
/// assert_eq!(probe.observe().await, Recv(Update{from:41, to:41}));
/// #
/// # Ok(())
/// # }
/// ```
pub struct Probe<I: MessagePlus, O: MessagePlus> {
    input_interceptor: mpsc::Sender<I>,
    input_receiver: mpsc::Receiver<I>,
    input_forwarder: DynSender<I>,
    output_interceptor: mpsc::Sender<O>,
    output_receiver: mpsc::Receiver<O>,
    output_forwarder: DynSender<O>,
}

/// An event observed by a [Probe](crate::test_helpers::Probe)
///
/// These events have to be interpreted from the point of view of
/// the actor on which the probe has been set.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ProbeEvent<I: MessagePlus, O: MessagePlus> {
    /// The observed actor received some input message
    Recv(I),

    /// The observed actor sent some output message
    Send(O),

    /// The input stream of the observed actor has been closed
    CloseRecv,

    /// The output stream of the observed actor has been closed
    CloseSend,

    /// The observed actor is fully disconnected
    Closed,
}

impl<I: MessagePlus, O: MessagePlus> Default for Probe<I, O> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: MessagePlus, O: MessagePlus> Probe<I, O> {
    /// Create a new `Probe` ready to be interposed between two actors.
    ///
    /// The connection is done using the [with_probe()](crate::test_helpers::ServiceConsumerExt::with_probe)
    /// method added to any [ServiceConsumer](crate::ServiceConsumer)
    /// by [ServiceConsumerExt](crate::test_helpers::ServiceConsumerExt).
    pub fn new() -> Self {
        // The capacity of the interceptor channels is 1,
        // so the probe will control at which pace input/output messages are sent.
        let (input_interceptor, input_receiver) = mpsc::channel(1);
        let (output_interceptor, output_receiver) = mpsc::channel(1);

        // Use null senders till this probe is connected to actual message boxes.
        let input_forwarder = NullSender.into();
        let output_forwarder = NullSender.into();

        Probe {
            input_interceptor,
            input_receiver,
            input_forwarder,
            output_interceptor,
            output_receiver,
            output_forwarder,
        }
    }

    /// Return the next event observed between the two connected actors.
    ///
    /// Note that calling `observe()` is mandatory for the actors to make progress.
    /// Indeed the observed channels are blocked on each action till the event is actually observed.
    /// Hence a probe also controls the pace at which input/output messages are sent
    /// over the observed connection between the two actors
    pub async fn observe(&mut self) -> ProbeEvent<I, O> {
        // Ensure that input/output can only be sent by the observed actors
        let _ = self.input_interceptor.close().await;
        let _ = self.output_interceptor.close().await;

        // Both input and output sender actors might have completed
        if self.input_receiver.is_terminated() && self.output_receiver.is_terminated() {
            return ProbeEvent::Closed;
        }

        // When the input sender has completed: focus on output
        if self.input_receiver.is_terminated() {
            let output = self.output_receiver.next().await;
            return self.notify_output(output).await;
        }

        // When the output sender has completed: focus on input
        if self.output_receiver.is_terminated() {
            let input = self.input_receiver.next().await;
            return self.notify_input(input).await;
        }

        // Notify either input or output depending which is first
        tokio::select! {
            input = self.input_receiver.next() => {
                self.notify_input(input).await
            },
            output = self.output_receiver.next() => {
                self.notify_output(output).await
            },
        }
    }

    async fn notify_input(&mut self, input: Option<I>) -> ProbeEvent<I, O> {
        match input {
            None => ProbeEvent::CloseRecv,
            Some(input) => {
                let event = input.clone();
                self.input_forwarder
                    .send(input)
                    .await
                    .expect("input to be forwarded");
                ProbeEvent::Recv(event)
            }
        }
    }

    async fn notify_output(&mut self, output: Option<O>) -> ProbeEvent<I, O> {
        match output {
            None => ProbeEvent::CloseSend,
            Some(output) => {
                let event = output.clone();
                self.output_forwarder
                    .send(output)
                    .await
                    .expect("output to be forwarded");
                ProbeEvent::Send(event)
            }
        }
    }
}

/// Extend any [ServiceConsumer] with a `with_probe` method.
pub trait ServiceConsumerExt<Request: MessagePlus, Response: MessagePlus> {
    /// Add a probe to an actor `self` that is a [ServiceConsumer](crate::ServiceConsumer).
    ///
    /// Return a [ServiceConsumer](crate::ServiceConsumer)
    /// that can be plugged into another actor which is a [ServiceProvider](crate::ServiceProvider).
    ///
    /// The added `Probe` is then interposed between the two actors,
    /// observing all the [ProbeEvent](crate::test_helpers::ProbeEvent) exchanged between them.
    ///
    /// ```
    /// # use tedge_actors::{NoConfig, ServerMessageBoxBuilder, SimpleMessageBoxBuilder};
    /// # use crate::tedge_actors::examples::calculator::*;
    /// use tedge_actors::test_helpers::Probe;               // The probe struct
    /// use tedge_actors::ServiceConsumer;                   // is a `ServiceConsumer`
    /// use tedge_actors::test_helpers::ServiceConsumerExt;  // Adds `.with_probe()`
    ///
    /// // Build the actor message boxes
    /// let mut server_box_builder : ServerMessageBoxBuilder<Operation, Update> = ServerMessageBoxBuilder::new("Calculator", 16);
    /// let mut client_box_builder = SimpleMessageBoxBuilder::new("Player 1", 1);
    ///
    /// // Connect the two actor message boxes interposing a probe.
    /// let mut probe = Probe::new();
    /// client_box_builder.with_probe(&mut probe).set_connection(&mut server_box_builder);
    /// ```
    fn with_probe<'a>(
        &'a mut self,
        probe: &'a mut Probe<Response, Request>,
    ) -> &'a mut Probe<Response, Request>;
}

impl<T, Request: MessagePlus, Response: MessagePlus> ServiceConsumerExt<Request, Response> for T
where
    T: ServiceConsumer<Request, Response, NoConfig>,
{
    fn with_probe<'a>(
        &'a mut self,
        probe: &'a mut Probe<Response, Request>,
    ) -> &'a mut Probe<Response, Request> {
        probe.input_forwarder = self.get_response_sender();
        self.set_request_sender(probe.output_interceptor.clone().into());
        probe
    }
}

impl<I: MessagePlus, O: MessagePlus> ServiceConsumer<O, I, NoConfig> for Probe<I, O> {
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn set_request_sender(&mut self, sender: DynSender<O>) {
        self.output_forwarder = sender;
    }

    fn get_response_sender(&self) -> DynSender<I> {
        self.input_interceptor.clone().into()
    }
}

impl<I: MessagePlus, O: MessagePlus> MessageSource<O, NoConfig> for Probe<I, O> {
    fn register_peer(&mut self, _config: NoConfig, sender: DynSender<O>) {
        self.output_forwarder = sender;
    }
}

impl<I: MessagePlus, O: MessagePlus> MessageSink<I, NoConfig> for Probe<I, O> {
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn get_sender(&self) -> DynSender<I> {
        self.input_interceptor.clone().into()
    }
}

pub trait ServiceProviderExt<I: Message, O: Message, C> {
    /// Create a simple message box connected to a box under construction.
    fn new_client_box(&mut self, config: C) -> SimpleMessageBox<O, I>;
}

impl<I, O, C, T> ServiceProviderExt<I, O, C> for T
where
    I: Message,
    O: Message,
    C: Clone,
    T: ServiceProvider<I, O, C>,
{
    fn new_client_box(&mut self, config: C) -> SimpleMessageBox<O, I> {
        let name = "client-box";
        let capacity = 16;
        let mut client_box = ConsumerBoxBuilder::new(name, capacity, config);
        self.add_peer(&mut client_box);
        client_box.build()
    }
}

struct ConsumerBoxBuilder<I, O: Debug, C> {
    config: C,
    box_builder: SimpleMessageBoxBuilder<O, I>,
}

impl<I: Message, O: Message, C> ConsumerBoxBuilder<I, O, C> {
    fn new(name: &str, capacity: usize, config: C) -> Self {
        ConsumerBoxBuilder {
            config,
            box_builder: SimpleMessageBoxBuilder::new(name, capacity),
        }
    }

    fn build(self) -> SimpleMessageBox<O, I> {
        self.box_builder.build()
    }
}

impl<I: Message, O: Message, C: Clone> ServiceConsumer<I, O, C> for ConsumerBoxBuilder<I, O, C> {
    fn get_config(&self) -> C {
        self.config.clone()
    }

    fn set_request_sender(&mut self, request_sender: DynSender<I>) {
        self.box_builder.set_request_sender(request_sender)
    }

    fn get_response_sender(&self) -> DynSender<O> {
        self.box_builder.get_response_sender()
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
