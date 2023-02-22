use crate::mpsc;
use crate::Builder;
use crate::DynSender;
use crate::Message;
use crate::MessageSink;
use crate::MessageSource;
use crate::NoConfig;
use crate::NullSender;
use crate::Sender;
use crate::ServiceConsumer;
use crate::ServiceProvider;
use crate::SimpleMessageBox;
use crate::SimpleMessageBoxBuilder;
use futures::stream::FusedStream;
use futures::SinkExt;
use futures::StreamExt;
use std::fmt::Debug;

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
/// # use crate::tedge_actors::examples::calculator::*;
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
/// tokio::spawn(ServerActor::new(Calculator::default()).run(server_box_builder.build()));
/// tokio::spawn(Player { name: "Player".to_string(), target: 42}.run(player_box_builder.build()));
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
    /// Hence a probe also control at which pace the input/output messages are sent
    /// between the two actors which connection is observed.
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
    /// Add a probe to `self` that is a [ServiceConsumer](crate::ServiceConsumer) of a first actor.
    ///
    /// Return a [ServiceConsumer](crate::ServiceConsumer)
    /// ready to be plug into a [ServiceProvider](crate::ServiceProvider) of a second actor.
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

impl<I: MessagePlus, O: MessagePlus> MessageSink<I> for Probe<I, O> {
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

struct ConsumerBoxBuilder<I, O, C> {
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
