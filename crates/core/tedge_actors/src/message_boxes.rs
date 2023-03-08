//! Message boxes are the only way for actors to interact with each others.
//!
//! When an [Actor](crate::Actor) instance is spawned,
//! this actor is given a [MessageBox](crate::MessageBox)
//! to collect its input [Messages](crate::Message) and to forward its output [Messages](crate::Message).
//!
//! Conceptually, a message box is a receiver of input messages combined with a sender of output messages.
//! * The receiver is connected to the senders of peer actors;
//!   and reciprocally the sender is connected to receivers of peer actors.
//! * The receivers are [mpsc::Receiver](crate::mpsc::Receiver) that collect messages from several sources,
//!   and deliver the messages to the actor in the order they have been received.
//! * The senders are [DynSender](crate::DynSender) that adapt the messages sent to match constraints of the receivers.
//!
//! A [SimpleMessageBox](crate::SimpleMessageBox) implements exactly this conceptual view:
//!
//! ```ascii
//!                    input_senders: DynSender<Input> ...
//!
//!                                   │
//!         ┌─────────────────────────┴───────────────────────────┐
//!         │                         ▼                           │
//!         │         input_receiver: mpsc::Receiver<Input>       │
//!         │                                                     │
//!         │                         │                           │
//!         │                         │                           │
//!         │                         ▼                           │
//!         │                    actor: Actor                     │
//!         │                                                     │
//!         │                         │                           │
//!         │                         │                           │
//!         │                         ▼                           │
//!         │          output_sender: DynSender<Output>           │
//!         │                                                     │
//!         └─────────────────────────┬───────────────────────────┘
//!                                   │
//!                                   ▼
//!                output_receivers: mpsc::Receiver<Output> ...
//! ```
//!
//! In practice, a message box can wrap more than a single receiver and sender.
//! Indeed, collecting all the messages in a single receiver, a single queue,
//! prevents the actor to process some messages with a higher priority,
//! something that is required to handle runtime requests
//! or to await a response from a specific service.
//!
//! Here is a typical message box that let the actor
//! - handles not only regular Input and Output messages
//! - but also processes runtime requests with a higher priority
//! - and awaits specifically for responses to its HTTP requests.
//!
//! ```ascii
//!
//!                     │                                      │
//! ┌───────────────────┴──────────────────────────────────────┴─────────────────────────┐
//! │                   ▼                                      ▼                         │
//! │   input_receiver: mpsc::Receiver<Input>     runtime: Receiver<RuntimeRequest>      │
//! │                   │                                                                │
//! │                   │                                                                │
//! │                   ▼                         http_request: DynSender<HttpRequest> ──┼────►
//! │              actor: Actor                                                          │
//! │                   │                        http_response: Receiver<HttpResponse> ◄─┼─────
//! │                   │                                                                │
//! │                   ▼                                                                │
//! │    output_sender: DynSender<Output>                                                │
//! │                                                                                    │
//! └───────────────────┬────────────────────────────────────────────────────────────────┘
//!                     │
//!                     ▼
//! ```
//!
//! To address this diversity of message priority requirements,
//! but also to add specific coordination among input and output channels,
//! each [Actor](crate::Actor) is free to choose its own [MessageBox](crate::MessageBox) implementation:
//!
//! ```no_run
//! # use crate::tedge_actors::MessageBox;
//! trait Actor {
//!     /// Type of message box used by this actor
//!     type MessageBox: MessageBox;
//! }
//! ```
//!
//! This crates provides several built-in message box implementations:
//!
//! - [SimpleMessageBox](crate::SimpleMessageBox) for actors that simply process messages in turn,
//! - [ServerMessageBox](crate::ServerMessageBox) for server actors that deliver a request-response service,
//! - [ConcurrentServerMessageBox](crate::ConcurrentServerMessageBox) for server actors that process requests concurrently,
//! - [ClientMessageBox](crate::ClientMessageBox) for client actors that use a request-response service from a server actor,
//!
//!

use crate::Builder;
use crate::ChannelError;
use crate::DynSender;
use crate::Message;
use crate::NoConfig;
use crate::RuntimeRequest;
use crate::ServiceConsumer;
use crate::ServiceProvider;
use crate::SimpleMessageBoxBuilder;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::StreamExt;
use log::debug;
use log::info;
use std::fmt::Debug;

/// A trait to define the interactions with a message box
///
pub trait MessageBox: 'static + Sized + Send + Sync {
    /// Type of input messages the actor consumes
    type Input: Message;

    /// Type of output messages the actor produces
    type Output: Message;

    // TODO: add a method aimed to build the box for testing purpose
    //       Without this its hard to relate the Input and Output messages of the box
    //       Currently we have on interface to a logger not a message box!
    // Build a message box along 2 channels to send and receive messages to and from the box
    // fn channel(name: &str, capacity: usize) -> ((DynSender<Self::Input>, DynReceiver<Self::Output>), Self);

    /// Turn on/off logging of input and output messages
    fn turn_logging_on(&mut self, on: bool);

    /// Name of the associated actor
    fn name(&self) -> &str;

    /// Log an input message just after reception, before processing it.
    fn log_input(&self, message: &impl Debug) {
        if self.logging_is_on() {
            info!(target: self.name(), "recv {:?}", message);
        }
    }

    /// Log an output message just before sending it.
    fn log_output(&self, message: &impl Debug) {
        if self.logging_is_on() {
            debug!(target: self.name(), "send {:?}", message);
        }
    }

    fn logging_is_on(&self) -> bool;
}

/// Either a message or a [RuntimeRequest]
pub enum WrappedInput<Input> {
    Message(Input),
    RuntimeRequest(RuntimeRequest),
}

#[async_trait]
pub trait ReceiveMessages<Input> {
    /// Return the next received message if any, returning [RuntimeRequest]'s as errors.
    /// Returning [RuntimeRequest] takes priority over messages.
    async fn try_recv(&mut self) -> Result<Option<Input>, RuntimeRequest>;

    /// Returns [Some] [WrappedInput] the next time a message is received. Returns [None] if
    /// the underlying channels are closed. Returning [RuntimeRequest] takes priority over messages.
    async fn recv_message(&mut self) -> Option<WrappedInput<Input>>;

    /// Returns [Some] message the next time a message is received. Returns [None] if
    /// both of the underlying channels are closed or if a [RuntimeRequest] is received.
    /// Handling [RuntimeRequest]'s by returning [None] takes priority over messages.
    async fn recv(&mut self) -> Option<Input>;
}

/// The basic message box
pub struct SimpleMessageBox<Input, Output> {
    name: String,
    input_receiver: CombinedReceiver<Input>,
    output_sender: DynSender<Output>,
    logging_is_on: bool,
}

impl<Input: Message, Output: Message> SimpleMessageBox<Input, Output> {
    pub fn new(
        name: String,
        input_receiver: CombinedReceiver<Input>,
        output_sender: DynSender<Output>,
    ) -> Self {
        SimpleMessageBox {
            name,
            input_receiver,
            output_sender,
            logging_is_on: true,
        }
    }

    pub async fn send(&mut self, message: Output) -> Result<(), ChannelError> {
        self.log_output(&message);
        self.output_sender.send(message).await
    }

    /// Close the sending channel of this message box.
    ///
    /// This makes the receiving end aware that no more message will be sent.
    pub fn close_output(&mut self) {
        self.output_sender.close_sender()
    }
}

#[async_trait]
impl<Input: Message, Output: Message> ReceiveMessages<Input> for SimpleMessageBox<Input, Output> {
    async fn try_recv(&mut self) -> Result<Option<Input>, RuntimeRequest> {
        self.input_receiver.try_recv().await
    }

    async fn recv_message(&mut self) -> Option<WrappedInput<Input>> {
        self.input_receiver.recv_message().await
    }

    async fn recv(&mut self) -> Option<Input> {
        self.input_receiver.recv().await.map(|message| {
            self.log_input(&message);
            message
        })
    }
}

pub struct CombinedReceiver<Input> {
    input_receiver: mpsc::Receiver<Input>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

impl<Input> CombinedReceiver<Input> {
    pub fn new(
        input_receiver: mpsc::Receiver<Input>,
        signal_receiver: mpsc::Receiver<RuntimeRequest>,
    ) -> Self {
        Self {
            input_receiver,
            signal_receiver,
        }
    }
}

#[async_trait]
impl<Input: Send> ReceiveMessages<Input> for CombinedReceiver<Input> {
    async fn try_recv(&mut self) -> Result<Option<Input>, RuntimeRequest> {
        match self.recv_message().await {
            Some(WrappedInput::Message(message)) => Ok(Some(message)),
            Some(WrappedInput::RuntimeRequest(runtime_request)) => Err(runtime_request),
            None => Ok(None),
        }
    }

    async fn recv_message(&mut self) -> Option<WrappedInput<Input>> {
        tokio::select! {
            biased;

            Some(runtime_request) = self.signal_receiver.next() => {
                Some(WrappedInput::RuntimeRequest(runtime_request))
            }
            Some(message) = self.input_receiver.next() => {
                Some(WrappedInput::Message(message))
            }
            else => None
        }
    }

    async fn recv(&mut self) -> Option<Input> {
        match self.recv_message().await {
            Some(WrappedInput::Message(message)) => Some(message),
            _ => None,
        }
    }
}

impl<Input: Message, Output: Message> MessageBox for SimpleMessageBox<Input, Output> {
    type Input = Input;
    type Output = Output;

    fn turn_logging_on(&mut self, on: bool) {
        self.logging_is_on = on;
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn logging_is_on(&self) -> bool {
        self.logging_is_on
    }
}

/// A message box for a request-response server
pub type ServerMessageBox<Request, Response> =
    SimpleMessageBox<(ClientId, Request), (ClientId, Response)>;

/// Internal id assigned to a client actor of a server actor
pub type ClientId = usize;

/// A message box for services that handles requests concurrently
pub struct ConcurrentServerMessageBox<Request, Response> {
    /// Max concurrent requests
    max_concurrency: usize,

    /// Message box to interact with clients of this service
    clients: ServerMessageBox<Request, Response>,

    /// Pending responses
    pending_responses: futures::stream::FuturesUnordered<PendingResult<(usize, Response)>>,
}

type PendingResult<R> = tokio::task::JoinHandle<R>;

impl<Request: Message, Response: Message> ConcurrentServerMessageBox<Request, Response> {
    pub(crate) fn new(
        max_concurrency: usize,
        clients: ServerMessageBox<Request, Response>,
    ) -> Self {
        ConcurrentServerMessageBox {
            max_concurrency,
            clients,
            pending_responses: futures::stream::FuturesUnordered::new(),
        }
    }

    pub async fn recv(&mut self) -> Option<(ClientId, Request)> {
        self.next_request().await
    }

    pub async fn send(&mut self, message: (ClientId, Response)) -> Result<(), ChannelError> {
        self.clients.send(message).await
    }

    async fn next_request(&mut self) -> Option<(usize, Request)> {
        self.await_idle_processor().await;
        loop {
            tokio::select! {
                Some(request) = self.clients.recv() => {
                    return Some(request);
                }
                Some(result) = self.pending_responses.next() => {
                    self.send_result(result).await;
                }
                else => {
                    return None
                }
            }
        }
    }

    async fn await_idle_processor(&mut self) {
        if self.pending_responses.len() >= self.max_concurrency {
            if let Some(result) = self.pending_responses.next().await {
                self.send_result(result).await;
            }
        }
    }

    pub fn send_response_once_done(&mut self, pending_result: PendingResult<(ClientId, Response)>) {
        self.pending_responses.push(pending_result);
    }

    async fn send_result(&mut self, result: Result<(usize, Response), tokio::task::JoinError>) {
        if let Ok(response) = result {
            let _ = self.clients.send(response).await;
        }
        // TODO handle error cases:
        // - cancelled task
        // - task panics
        // - send fails
    }
}

impl<Request: Message, Response: Message> MessageBox
    for ConcurrentServerMessageBox<Request, Response>
{
    type Input = (ClientId, Request);
    type Output = (ClientId, Response);

    fn turn_logging_on(&mut self, on: bool) {
        self.clients.turn_logging_on(on)
    }

    fn name(&self) -> &str {
        self.clients.name()
    }

    fn logging_is_on(&self) -> bool {
        self.clients.logging_is_on()
    }
}

/// Client side handler of requests/responses sent to an actor
///
/// Note that this message box sends requests and receive responses.
pub struct ClientMessageBox<Request, Response> {
    messages: SimpleMessageBox<Response, Request>,
}

impl<Request: Message, Response: Message> ClientMessageBox<Request, Response> {
    /// Create a new `ClientMessageBox` connected to the service.
    pub fn new(
        client_name: &str,
        service: &mut impl ServiceProvider<Request, Response, NoConfig>,
    ) -> Self {
        let capacity = 1; // At most one response is ever expected
        let messages = SimpleMessageBoxBuilder::new(client_name, capacity)
            .with_connection(service)
            .build();
        ClientMessageBox { messages }
    }

    /// Send the request and await for a response
    pub async fn await_response(&mut self, request: Request) -> Result<Response, ChannelError> {
        self.messages.send(request).await?;
        self.messages
            .recv()
            .await
            .ok_or(ChannelError::ReceiveError())
    }
}

impl<Request: Message, Response: Message> MessageBox for ClientMessageBox<Request, Response> {
    type Input = Response;
    type Output = Request;

    fn turn_logging_on(&mut self, on: bool) {
        self.messages.turn_logging_on(on)
    }

    fn name(&self) -> &str {
        self.messages.name()
    }

    fn logging_is_on(&self) -> bool {
        self.messages.logging_is_on()
    }
}
