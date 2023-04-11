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
//! - [Builder](crate::Builder):
//!   this trait defines how to build the actor message box once fully connected to its peers.
//! - [MessageSink](crate::MessageSink):
//!   declares that the message box under construction can receive input messages.
//! - [MessageSource](crate::MessageSource):
//!   declares that the message box under construction is a source of output messages
//!   to which an actor can subscribe to providing some
//! - [ServiceProvider](crate::ServiceProvider)
//!   declares that the message box under construction is that of a service provider.
//!   This service expects inputs and returns outputs of a specific type,
//!   and might requires the consumers to provide subscription config.
//! - [ServiceConsumer](crate::ServiceConsumer):
//!   declares that the message box under construction is that of a service consumer.
//!
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

/// The builder of a MessageBox must implement this trait for every message type that can be sent to it
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
    /// # use tedge_actors::test_helpers::MessageReceiverExt;
    ///
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

/// The builder of a MessageBox must implement this trait for every message type that it can receive from its peers
pub trait MessageSource<M: Message, Config> {
    /// The message will be sent to the peer using the provided `sender`
    fn register_peer(&mut self, config: Config, sender: DynSender<M>);

    /// Connect a peer actor that will consume the message produced by this actor
    fn add_sink(&mut self, peer: &impl MessageSink<M, Config>) {
        self.register_peer(peer.get_config(), peer.get_sender());
    }
}

/// The builder of a MessageBox must implement this trait to receive requests from the runtime
pub trait RuntimeRequestSink {
    /// Return the sender that can be used by the runtime to send requests to this actor
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest>;
}

/// A trait to connect a message box under-construction to peer messages boxes
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

/// A connection port to connect a message box under-connection to another box
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

/// A builder of SimpleMessageBox
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

impl<I: Message, O: Message, C> MessageSource<O, C> for SimpleMessageBoxBuilder<I, O> {
    fn register_peer(&mut self, _config: C, sender: DynSender<O>) {
        self.output_sender = sender;
    }
}

impl<I: Message, O: Message> MessageSink<I, NoConfig> for SimpleMessageBoxBuilder<I, O> {
    fn get_config(&self) -> NoConfig {
        NoConfig
    }

    fn get_sender(&self) -> DynSender<I> {
        self.input_sender.sender_clone()
    }
}

impl<I: Message, O: Message> RuntimeRequestSink for SimpleMessageBoxBuilder<I, O> {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.signal_sender.sender_clone()
    }
}

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
