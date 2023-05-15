//! Actor and Message box builders
//!
//! [Actor](crate::Actor) implementations are given the freedom
//! to choose their own [message box](crate::message_boxes) implementation.
//! This adds the flexibility to process some messages with a higher priority,
//! to await a message from a specific source,
//! or simply to ease the actor code with specific peer handles.
//!
//! However, only the *internal* structure and usage of a message box is let free to each actor.
//! The *external* view of a message box is standardized, so actors can be connected to each others,
//! with no knowledge of their internal organisation.
//!
//! In order to let peer actors connect to its message box,
//! an actor implementation must provide a message box builder
//! that defines the various connection points using the following traits:
//!
//! - [Builder]:
//!   defines how to build the actor and its message box once fully connected to its peers.
//! - [MessageSink]:
//!   declares that the actor under construction expect input messages of a given type,
//!   and tells how to connect a [MessageSource] to receive those.
//! - [MessageSource]
//!   declares that the actor under construction is a source of output messages,
//!   and tells how to connect a [MessageSink] to which the messages will be directed.
//! - [ServiceProvider]
//!   declares that the actor under construction is a service provider,
//!   that produces output messages as a reaction to input messages.
//! - [ServiceConsumer]:
//!   declares that the actor under construction depends on a service provider,
//!   and tells how to connect such a [ServiceProvider] to interact with.
//! - [RuntimeRequestSink]:
//!   defines how the runtime can connect the actor under construction.
//!
//! In practice:
//!
//! - An actor builder has to implement at least the [Builder] and [RuntimeRequestSink] traits,
//!   so the runtime can connect itself to the actor and run it,
//!   using its [spawn](crate::Runtime::spawn) method.
//! - An actor builder that *depends* on some service provided by a [ServiceProvider],
//!   *must* implement the [ServiceConsumer] trait for the input, output and config types
//!   defined by the provider.
//! - Similarly, if an actor needs to connect a [MessageSource],
//!   its builder must implement the [MessageSink] trait with the appropriate message and config types.
//! - Vice versa, if an actor needs to send messages to some [MessageSink],
//!   its builder must implement the [MessageSource] trait with the appropriate message and config types.
//! - In order to define its input and output, an actor builder implements *either* the [ServiceProvider] trait
//!   or the [MessageSource] and [MessageSink] traits.
//! - An actor builder implements the [MessageSource] and [MessageSink] traits
//!   when it makes sense for a peer to *only* send messages to or to *only* receive messages
//!   from the actor under construction.
//! - An actor builder implements the [ServiceProvider] trait
//!   when there is a strong request-response relationship between the messages sent and received,
//!   the responses being meaningful only for the actor sending the triggering requests.
//!
//! An actor builder can use a [SimpleMessageBoxBuilder] to ease all these implementations.
//!
//! ## Rationale
//!
//! Here are the keys to understand how these traits are designed and used.
//!
//! - The main point is to establish [mpsc channels](futures::channel::mpsc) between actors.
//!   Each actor owns a Receiver to gather all its inputs
//!   (possibly several receivers to handle message priorities among inputs),
//!   and has to give clones of the associated Sender (or Senders) to its peers.
//! - The first responsibility of a builder is to create a channel per receiver of the actor
//!   under construction. The receiver will be given to the actor on build.
//!   The sender is owned by the builder to be cloned and given to any peer that needs to send data
//!   to the actor under construction.
//! - The second responsibility of the builder is to collect a Sender for each peer the actor
//!   under construction needs to send messages to. This is the mirror of the previous responsibility:
//!   each builder gives to the others clones of its senders and collects senders from others.
//! - This is why all the actor building traits
//!   ([MessageSource], [MessageSink], [ServiceProvider], [ServiceConsumer] and [RuntimeRequestSink])
//!   are related to exchanges of Sender. A sink gives to a source a sender attached to its receiver.
//! - To be precise, the actor builders exchange [DynSender] and not [Sender]. The difference is that
//!   a [DynSender] can transform the messages sent by the source to adapt them to the sink expectations,
//!   using an `impl From<SourceMessage> for SinkMessage`. This flexibility allows an actor to receive
//!   messages from several independent sources (see the [fan_in_message_type](crate::fan_in_message_type) macro).
use crate::mpsc;
use crate::DynSender;
use crate::LoggingReceiver;
use crate::LoggingSender;
use crate::MappingSender;
use crate::Message;
use crate::NullSender;
use crate::RuntimeRequest;
use crate::Sender;
use crate::SimpleMessageBox;
use std::convert::Infallible;
use std::fmt::Debug;

/// Builder of `T`
///
/// In practice, this trait is used to implement [Actor](crate::Actor) builders.
pub trait Builder<T>: Sized {
    type Error: std::error::Error;

    /// Builds the entity or returns an error
    fn try_build(self) -> Result<T, Self::Error>;

    /// Builds the entity or panics
    fn build(self) -> T {
        self.try_build().unwrap()
    }
}

/// Placeholder when no specific config is required by a builder implementation
#[derive(Clone)]
pub struct NoConfig;

/// The [Builder] of an [Actor](crate::Actor) must implement this trait
/// for every message type that actor can receive from its peers.
///
/// An actor whose builder is a `MessageSink<M, C>` can be connected to any other actor
/// whose builder is a `MessageSource<M, C>` so that the sink can receive messages from that source.
///
/// A sink might be interested only in a subset of the messages emitted by the source.
/// For that purpose each source implementation defines a `Config` type parameter,
/// and the sink has to provide the configuration value specific to its needs.
pub trait MessageSink<M: Message, Config> {
    /// Return the config used by this actor to connect the message source
    fn get_config(&self) -> Config;

    /// Return the sender that can be used by peers to send messages to this actor
    fn get_sender(&self) -> DynSender<M>;

    /// Add a source of messages to the actor under construction
    fn add_input<N>(&mut self, source: &mut impl MessageSource<N, Config>)
    where
        N: Message,
        M: From<N>,
    {
        source.register_peer(self.get_config(), crate::adapt(&self.get_sender()))
    }

    /// Add a source of messages to the actor under construction, the messages being translated on the fly.
    ///
    /// The transformation function will be applied to the messages sent by the source,
    /// to convert them in a sequence, possibly empty, of messages forwarded to this sink.
    ///
    /// ```
    /// # use std::time::Duration;
    /// # use tedge_actors::Builder;
    /// # use tedge_actors::ChannelError;
    /// # use tedge_actors::MessageReceiver;
    /// # use tedge_actors::MessageSink;
    /// # use tedge_actors::NoMessage;
    /// # use tedge_actors::Sender;
    /// # use tedge_actors::SimpleMessageBox;
    /// # use tedge_actors::SimpleMessageBoxBuilder;
    /// # #[cfg(feature = "test-helpers")]
    /// # use tedge_actors::test_helpers::MessageReceiverExt;
    ///
    /// #[cfg(feature = "test-helpers")]
    /// # #[tokio::main]
    /// # async fn main() -> Result<(),ChannelError> {
    /// let mut receiver_builder = SimpleMessageBoxBuilder::new("Recv", 16);
    /// let mut sender_builder = SimpleMessageBoxBuilder::new("Send", 16);
    ///
    /// // Convert the `&str` sent by the source into an iterator of `char` as expected by the receiver.
    /// receiver_builder.add_mapped_input(&mut sender_builder, |str: &'static str| str.chars() );
    ///
    /// let mut sender: SimpleMessageBox<NoMessage, &'static str>= sender_builder.build();
    /// let receiver: SimpleMessageBox<char, NoMessage> = receiver_builder.build();
    ///
    /// sender.send("Hello!").await?;
    ///
    /// let mut receiver = receiver.with_timeout(Duration::from_millis(100));
    /// assert_eq!(receiver.recv().await, Some('H'));
    /// assert_eq!(receiver.recv().await, Some('e'));
    /// assert_eq!(receiver.recv().await, Some('l'));
    /// assert_eq!(receiver.recv().await, Some('l'));
    /// assert_eq!(receiver.recv().await, Some('o'));
    /// assert_eq!(receiver.recv().await, Some('!'));
    /// assert_eq!(receiver.recv().await, None);
    ///
    /// # Ok(())
    /// # }
    /// ```
    fn add_mapped_input<N, MS, MessageMapper>(
        &mut self,
        source: &mut impl MessageSource<N, Config>,
        cast: MessageMapper,
    ) where
        N: Message,
        MS: Iterator<Item = M> + Send,
        MessageMapper: Fn(N) -> MS,
        MessageMapper: 'static + Send + Sync,
    {
        let sender = MappingSender::new(self.get_sender(), cast);
        source.register_peer(self.get_config(), sender.into())
    }
}

/// The [Builder] of an [Actor](crate::Actor) must implement this trait
/// for every message type that actor can send to its peers.
///
/// To receive messages from a `MessageSource<M, C>`, the peer must be a `MessageSink<M, C>`.
pub trait MessageSource<M: Message, Config> {
    /// The message will be sent to the peer using the provided `sender`
    fn register_peer(&mut self, config: Config, sender: DynSender<M>);

    /// Connect a peer actor that will consume the message produced by this actor
    fn add_sink(&mut self, peer: &impl MessageSink<M, Config>) {
        self.register_peer(peer.get_config(), peer.get_sender());
    }
}

/// The [Builder] of an [Actor](crate::Actor) must implement this trait
/// to receive [runtime requests](crate::RuntimeRequest]s like shutdown requests from the [Runtime](crate::Runtime).
pub trait RuntimeRequestSink {
    /// Return the sender that can be used by the runtime to send requests to this actor
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest>;
}

/// A trait that defines that an actor provides a service
/// by accepting `Request` messages and sending `Response` from/to its peers.
///
/// In order to connect to a to `ServiceProvider<Req, Res, Conf>` and avail its services,
/// the peer must be a `ServiceConsumer<Req, Res, Conf>`.
///
/// The config parameter is typically used by the `ServiceConsumer`
/// to register any message filtering criteria to the `ServiceProvider`.
pub trait ServiceProvider<Request: Message, Response: Message, Config> {
    /// Connect a peer message box to the message box under construction
    fn add_peer(&mut self, peer: &mut impl ServiceConsumer<Request, Response, Config>) {
        let config = peer.get_config();
        let response_sender = peer.get_response_sender();
        let request_sender = self.connect_consumer(config, response_sender);
        peer.set_request_sender(request_sender);
    }

    /// Connect a consumer to the service provider under construction
    /// returning to that service consumer a sender for its requests.
    ///
    /// The consumer provides:
    /// - a config to filter the responses of interest,
    /// - a sender where the responses will have to be sent by the service.
    ///
    /// The consumer is given back:
    /// - a sender where its requests will have to be sent to the service.
    fn connect_consumer(
        &mut self,
        config: Config,
        response_sender: DynSender<Response>,
    ) -> DynSender<Request>;
}

/// A trait that defines that the actor under-construction
/// is a consumer of the service provided by another actor that is a `ServiceProvider`.
///
/// A `ServiceConsumer<Req, Res, Conf>` actor can be connected to another actor as its peer
/// if that actor is a `ServiceProvider<Req, Res, Conf>`.
pub trait ServiceConsumer<Request: Message, Response: Message, Config> {
    /// Return the config used by this actor to connect the service provider
    fn get_config(&self) -> Config;

    /// Set the sender to be used by this actor's box to send requests
    fn set_request_sender(&mut self, request_sender: DynSender<Request>);

    /// Return a sender where the responses to this actor's box have to be sent
    fn get_response_sender(&self) -> DynSender<Response>;

    /// Connect this client message box to the service message box
    fn set_connection(
        &mut self,
        service: &mut impl ServiceProvider<Request, Response, Config>,
    ) -> &mut Self
    where
        Self: Sized,
    {
        service.add_peer(self);
        self
    }

    /// Connect this client message box to the service message box
    ///
    /// Return the updated client message box.
    #[must_use]
    fn with_connection(
        mut self,
        service: &mut impl ServiceProvider<Request, Response, Config>,
    ) -> Self
    where
        Self: Sized,
    {
        service.add_peer(&mut self);
        self
    }
}

/// A [Builder] of [SimpleMessageBox]
///
/// This builder can be used as a building block for actor builders
/// as most actors use a [SimpleMessageBox] or a similar message box.
///
/// ```
/// # use std::convert::Infallible;
/// # use tedge_actors::{Builder, DynSender, RuntimeRequest, RuntimeRequestSink, SimpleMessageBox, SimpleMessageBoxBuilder};
/// # struct MyActorState (i64);
/// # type MyActorConfig = i64;
/// # type MyActorInput = i64;
/// # type MyActorOutput = i64;
/// # impl MyActorState {
/// #    pub fn new(config: MyActorConfig) -> MyActorState {
/// #        MyActorState(config)
/// #    }
/// # }
/// struct MyActor {
///    state: MyActorState,
///    messages: SimpleMessageBox<MyActorInput, MyActorOutput>,
/// }
///
/// struct MyActorBuilder {
///    config: MyActorConfig,
///    messages: SimpleMessageBoxBuilder<MyActorInput, MyActorOutput>,
/// }
///
/// impl Builder<MyActor> for MyActorBuilder {
///     type Error = Infallible;
///
///     fn try_build(self) -> Result<MyActor, Self::Error> {
///        Ok(self.build())
///     }
///
///     fn build(self) -> MyActor {
///         let state = MyActorState::new(self.config);
///         let messages = self.messages.build();
///         MyActor { state, messages }
///     }
/// }
/// ```
///
/// A [SimpleMessageBox] can be connected to the runtime.
/// An actor receiving its main input from a `SimpleMessageBox`
/// can receive `RuntimeRequest`s  as well from the same message box,
/// by making the actor builder implement [RuntimeRequestSink] for that actor.
///
/// ```
/// # use tedge_actors::{DynSender, RuntimeRequest, RuntimeRequestSink, SimpleMessageBoxBuilder};
/// # type MyActorConfig = i64;
/// # type MyActorInput = i64;
/// # type MyActorOutput = i64;
/// struct MyActorBuilder {
///    config: MyActorConfig,
///    messages: SimpleMessageBoxBuilder<MyActorInput, MyActorOutput>,
/// }
///
/// impl RuntimeRequestSink for MyActorBuilder {
///     fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
///         self.messages.get_signal_sender()
///     }
/// }
/// ```
///
/// Similarly, as a `SimpleMessageBoxBuilder` is a [ServiceProvider], this can be used
/// to implement the [ServiceProvider] trait for an actor using a [SimpleMessageBox] for its main input.
///
/// ```
/// # use tedge_actors::{DynSender, NoConfig, ServiceConsumer, ServiceProvider, SimpleMessageBoxBuilder};
/// # type MyActorConfig = i64;
/// # type MyActorInput = i64;
/// # type MyActorOutput = i64;
/// struct MyActorBuilder {
///    config: MyActorConfig,
///    messages: SimpleMessageBoxBuilder<MyActorInput, MyActorOutput>,
/// }
///
/// impl ServiceProvider<MyActorInput, MyActorOutput, NoConfig> for MyActorBuilder {
///     fn connect_consumer(
///        &mut self,
///        config: NoConfig,
///        response_sender: DynSender<MyActorOutput>)
///     -> DynSender<MyActorInput> {
///         self.messages.connect_consumer(config, response_sender)
///     }
/// }
/// ```
///
/// A notable use of [SimpleMessageBox] is for testing.
/// As a `SimpleMessageBoxBuilder` is a [ServiceConsumer]
/// one can use such a builder to connect and test an actor that is a [ServiceProvider].
///
/// Similarly:
/// - A `SimpleMessageBoxBuilder` is a [ServiceProvider] and can be used to test an actor that is a [ServiceConsumer].
/// - A `SimpleMessageBoxBuilder` is a [MessageSource] and can be used to test an actor that is a [MessageSink].
/// - A `SimpleMessageBoxBuilder` is a [MessageSink] and can be used to test an actor that is a [MessageSource].
///
/// ```
/// # use std::convert::Infallible;
/// # use tedge_actors::{Actor, Builder, DynSender, MessageReceiver, NoConfig, RuntimeError, Sender, ServiceConsumer, ServiceProvider, SimpleMessageBox, SimpleMessageBoxBuilder};
/// # struct MyActorState (i64);
/// # type MyActorConfig = i64;
/// # type MyActorInput = i64;
/// # type MyActorOutput = i64;
/// # impl MyActorState {
/// #    pub fn new(config: MyActorConfig) -> MyActorState {
/// #        MyActorState(config)
/// #    }
/// # }
/// # struct MyActor {
/// #    state: MyActorState,
/// #    messages: SimpleMessageBox<MyActorInput, MyActorOutput>,
/// # }
/// # struct MyActorBuilder {
/// #    config: MyActorConfig,
/// #    messages: SimpleMessageBoxBuilder<MyActorInput, MyActorOutput>,
/// # }
/// # impl MyActorBuilder {
/// #     pub fn new(config: MyActorConfig) -> MyActorBuilder {
/// #         let messages = SimpleMessageBoxBuilder::new("MyActor", 16);
/// #         MyActorBuilder { config, messages }
/// #     }
/// # }
/// # impl ServiceProvider<MyActorInput, MyActorOutput, NoConfig> for MyActorBuilder {
/// #    fn connect_consumer(&mut self, config: NoConfig, response_sender: DynSender<MyActorOutput>) -> DynSender<MyActorInput> {
/// #        self.messages.connect_consumer(config, response_sender)
/// #    }
/// # }
/// # impl Builder<MyActor> for MyActorBuilder {
/// #     type Error = Infallible;
/// #     fn try_build(self) -> Result<MyActor, Self::Error> {
/// #        Ok(self.build())
/// #     }
/// #     fn build(self) -> MyActor {
/// #         let state = MyActorState::new(self.config);
/// #         let messages = self.messages.build();
/// #         MyActor { state, messages }
/// #     }
/// # }
/// #[async_trait::async_trait]
/// impl Actor for MyActor {
///     fn name(&self) -> &str {
///         "My Actor"
///     }
///
///     async fn run(&mut self) -> Result<(), RuntimeError> {
///         while let Some(input) = self.messages.recv().await {
///             let output = input * 2;
///             self.messages.send(output).await?;
///         }
///         Ok(())
///     }
/// }
///
/// # #[tokio::main]
/// # async fn main() -> Result<(), RuntimeError> {
/// // Connect a test box to an actor under test
/// let mut my_actor_builder = MyActorBuilder::new(MyActorConfig::default());
/// let mut test_box_builder = SimpleMessageBoxBuilder::new("Test box", 16);
/// my_actor_builder.add_peer(&mut test_box_builder);
///
/// // Build the test box and run the actor
/// let mut test_box = test_box_builder.build();
/// let mut my_actor = my_actor_builder.build();
/// tokio::spawn(async move { my_actor.run().await } );
///
/// // any message sent by the test box is received by the actor under test
/// test_box.send(42).await?;
///
/// // any message sent by the actor under test is received by the test box
/// assert_eq!(test_box.recv().await, Some(84));
///
/// # Ok(())
/// # }
/// ```
///
pub struct SimpleMessageBoxBuilder<I: Debug, O> {
    name: String,
    input_sender: mpsc::Sender<I>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    output_sender: DynSender<O>,
    input_receiver: LoggingReceiver<I>,
}

impl<I: Message, O: Message> SimpleMessageBoxBuilder<I, O> {
    pub fn new(name: &str, capacity: usize) -> Self {
        let (input_sender, input_receiver) = mpsc::channel(capacity);
        let (signal_sender, signal_receiver) = mpsc::channel(4);
        let output_sender = NullSender.into();
        let input_receiver =
            LoggingReceiver::new(name.to_string(), input_receiver, signal_receiver);

        SimpleMessageBoxBuilder {
            name: name.to_string(),
            input_sender,
            signal_sender,
            output_sender,
            input_receiver,
        }
    }
}

/// A `SimpleMessageBoxBuilder<Request,Response>` is a [ServiceProvider]
/// accepting `Request` and sending back `Response`, with no specific config.
impl<Req: Message, Res: Message, Config> ServiceProvider<Req, Res, Config>
    for SimpleMessageBoxBuilder<Req, Res>
{
    fn connect_consumer(
        &mut self,
        _config: Config,
        response_sender: DynSender<Res>,
    ) -> DynSender<Req> {
        self.output_sender = response_sender;
        self.input_sender.sender_clone()
    }
}

/// A `SimpleMessageBoxBuilder<Request,Response>` is a [ServiceConsumer]
/// sending `Request` and expecting back `Response`, with no specific config.
impl<Req: Message, Res: Message> ServiceConsumer<Req, Res, NoConfig>
    for SimpleMessageBoxBuilder<Res, Req>
{
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn set_request_sender(&mut self, request_sender: DynSender<Req>) {
        self.output_sender = request_sender;
    }

    fn get_response_sender(&self) -> DynSender<Res> {
        self.input_sender.sender_clone()
    }
}

/// A `SimpleMessageBoxBuilder<Input,Output>` is a [MessageSource] of `Output` messages ignoring the config.
impl<I: Message, O: Message, C> MessageSource<O, C> for SimpleMessageBoxBuilder<I, O> {
    fn register_peer(&mut self, _config: C, sender: DynSender<O>) {
        self.output_sender = sender;
    }
}

/// A `SimpleMessageBoxBuilder<Input,Output>` is a [MessageSink] of `Input` messages with no specific config.
impl<I: Message, O: Message> MessageSink<I, NoConfig> for SimpleMessageBoxBuilder<I, O> {
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn get_sender(&self) -> DynSender<I> {
        self.input_sender.sender_clone()
    }
}

/// A `SimpleMessageBoxBuilder<Input,Output>` implements [RuntimeRequestSink] so the [Runtime](crate::Runtime)
/// can connect the message box under construction to send [runtime requests](crate::RuntimeRequest).
impl<I: Message, O: Message> RuntimeRequestSink for SimpleMessageBoxBuilder<I, O> {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.signal_sender.sender_clone()
    }
}

/// A `SimpleMessageBoxBuilder<Input,Output>` is a [Builder] of `SimpleMessageBox<Input,Output>`.
impl<Req: Message, Res: Message> Builder<SimpleMessageBox<Req, Res>>
    for SimpleMessageBoxBuilder<Req, Res>
{
    type Error = Infallible;

    fn try_build(self) -> Result<SimpleMessageBox<Req, Res>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> SimpleMessageBox<Req, Res> {
        let sender = LoggingSender::new(self.name, self.output_sender);
        SimpleMessageBox::new(self.input_receiver, sender)
    }
}
