//! Message boxes are the only way for actors to interact with each others.
//!
//! When an [Actor](crate::Actor) instance is spawned,
//! this actor is given a [message box](crate::message_boxes)
//! to collect its input [Messages](crate::Message) and to forward its output [Messages](crate::Message).
//!
//! Conceptually, a message box is a receiver of input messages combined with a sender of output messages.
//! * The receiver is connected to the senders of peer actors;
//!   and reciprocally the sender is connected to receivers of peer actors.
//! * The receivers are [mpsc::Receiver] that collect messages from several sources,
//!   and deliver the messages to the actor in the order they have been received.
//! * The senders are [DynSender] that adapt the messages sent to match constraints of the receivers.
//!
//! A [SimpleMessageBox] implements exactly this conceptual view:
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
//! each [Actor](crate::Actor) is free to choose its own [message box](crate::message_boxes) implementation:
//!
//! This crates provides several built-in message box implementations:
//!
//! - [SimpleMessageBox] for actors that simply process messages in turn,
//! - [ServerMessageBox](crate::ServerMessageBox) for server actors that deliver a request-response service,
//! - [ConcurrentServerMessageBox](crate::ConcurrentServerMessageBox) for server actors that process requests concurrently,
//! - [ClientMessageBox](crate::ClientMessageBox) for client actors that use a request-response service from a server actor,
//!
//!
//! ## Implementing specific message boxes
//!
//! TODO
//!
use crate::channels::Sender;
use crate::ChannelError;
use crate::CloneSender;
use crate::DynSender;
use crate::Message;
use crate::RuntimeRequest;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::StreamExt;
use log::debug;
use std::fmt::Debug;
use tracing::instrument;

#[async_trait]
pub trait MessageReceiver<Input> {
    /// Return the next received message if any, returning [RuntimeRequest]'s as errors.
    /// Returning [RuntimeRequest] takes priority over messages.
    async fn try_recv(&mut self) -> Result<Option<Input>, RuntimeRequest>;

    /// Returns [Some] message the next time a message is received. Returns [None] if
    /// both of the underlying channels are closed or if a [RuntimeRequest] is received.
    /// Handling [RuntimeRequest]'s by returning [None] takes priority over messages.
    async fn recv(&mut self) -> Option<Input>;

    /// Return [Some] [RuntimeRequest] if any is sent by the runtime,
    /// postponing the reception of regular messages while awaiting for [RuntimeRequest].
    async fn recv_signal(&mut self) -> Option<RuntimeRequest>;
}

pub struct LoggingReceiver<Input: Debug> {
    name: String,
    receiver: CombinedReceiver<Input>,
}

impl<Input: Debug> LoggingReceiver<Input> {
    pub fn new(
        name: String,
        input_receiver: mpsc::Receiver<Input>,
        signal_receiver: mpsc::Receiver<RuntimeRequest>,
    ) -> Self {
        let receiver = CombinedReceiver::new(input_receiver, signal_receiver);
        Self { name, receiver }
    }

    /// Splits a `LoggingReceiver` into an input receiver and a signal receiver,
    /// which can be used to read and write the stream concurrently.
    ///
    /// This method is more efficient than [`into_split`](Self::into_split), but
    /// the halves cannot be moved into independently spawned tasks.
    pub fn split(
        &mut self,
    ) -> (
        &mut mpsc::Receiver<Input>,
        &mut mpsc::Receiver<RuntimeRequest>,
    ) {
        (
            &mut self.receiver.input_receiver,
            &mut self.receiver.signal_receiver,
        )
    }

    /// Splits a `LoggingReceiver` into an input receiver and a signal receiver,
    /// which can be used to read and write the stream concurrently.
    ///
    /// This method returns consumes the `LoggingReceiver` and returns owned
    /// receivers, which can then be separately moved.
    pub fn into_split(self) -> (mpsc::Receiver<Input>, mpsc::Receiver<RuntimeRequest>) {
        (self.receiver.input_receiver, self.receiver.signal_receiver)
    }

    /// Close the input so no new messages can be sent to this receiver
    pub fn close_input(&mut self) {
        self.receiver.close_input();
    }
}

#[async_trait]
impl<Input: Send + Debug> MessageReceiver<Input> for LoggingReceiver<Input> {
    async fn try_recv(&mut self) -> Result<Option<Input>, RuntimeRequest> {
        let message = self.receiver.try_recv().await;
        debug!("recv {:?}", message);
        message
    }

    #[instrument(name = "LoggingReceiver::recv", skip(self), fields(name = self.name))]
    async fn recv(&mut self) -> Option<Input> {
        debug!("attempting recv");
        let message = self.receiver.recv().await;
        debug!("recv");
        message
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        let message = self.receiver.recv_signal().await;
        debug!(target: &self.name, "recv {:?}", message);
        message
    }
}

pub struct LoggingSender<Output> {
    name: String,
    sender: DynSender<Output>,
}

impl<Output: 'static> Clone for LoggingSender<Output> {
    fn clone(&self) -> Self {
        LoggingSender {
            name: self.name.clone(),
            sender: self.sender.sender_clone(),
        }
    }
}

impl<Output> LoggingSender<Output> {
    pub fn new(name: String, sender: DynSender<Output>) -> Self {
        Self { name, sender }
    }
}

#[async_trait]
impl<Output: Message> Sender<Output> for LoggingSender<Output> {
    #[instrument(name = "LoggingSender::send", skip(self, message), fields(name = self.name))]
    async fn send(&mut self, message: Output) -> Result<(), ChannelError> {
        log_message_sent(&self.name, &message);
        self.sender.send(message).await
    }
}

pub fn log_message_sent<I: Debug>(target: &str, message: I) {
    debug!(target: target, "send {message:?}");
}

/// An unbounded receiver
///
/// At least one such receiver should be used when there is a loop of actors,
/// say A sending data to B and B sending data to A.
pub struct UnboundedLoggingReceiver<Input: Debug> {
    name: String,
    input_receiver: mpsc::UnboundedReceiver<Input>,
    signal_receiver: mpsc::Receiver<RuntimeRequest>,
}

impl<Input: Debug> UnboundedLoggingReceiver<Input> {
    pub fn new(
        name: String,
        input_receiver: mpsc::UnboundedReceiver<Input>,
        signal_receiver: mpsc::Receiver<RuntimeRequest>,
    ) -> Self {
        Self {
            name,
            input_receiver,
            signal_receiver,
        }
    }

    async fn next_message(&mut self) -> Result<Option<Input>, RuntimeRequest> {
        tokio::select! {
            biased;

            Some(runtime_request) = self.signal_receiver.next() => {
                Err(runtime_request)
            }
            Some(message) = self.input_receiver.next() => {
                Ok(Some(message))
            }
            else => Ok(None)
        }
    }
}

#[async_trait]
impl<Input: Send + Debug> MessageReceiver<Input> for UnboundedLoggingReceiver<Input> {
    async fn try_recv(&mut self) -> Result<Option<Input>, RuntimeRequest> {
        let message = self.next_message().await;
        debug!(target: &self.name, "recv {:?}", message);
        message
    }

    async fn recv(&mut self) -> Option<Input> {
        let message = match self.next_message().await {
            Ok(Some(message)) => Some(message),
            _ => None,
        };
        debug!(target: &self.name, "recv {:?}", message);
        message
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        let message = self.signal_receiver.next().await;
        debug!(target: &self.name, "recv {:?}", message);
        message
    }
}

/// The basic message box to send and receive messages
///
/// - Handle runtime messages along regular ones
/// - Log received messages when pull out of the box
/// - Log sent messages when pushed into the target box
///
/// Such a box is connected to peer actors using a [SimpleMessageBoxBuilder](crate::SimpleMessageBoxBuilder).
pub struct SimpleMessageBox<Input: Debug, Output: Debug> {
    input_receiver: LoggingReceiver<Input>,
    output_sender: LoggingSender<Output>,
}

impl<Input: Message, Output: Message> SimpleMessageBox<Input, Output> {
    pub fn new(
        input_receiver: LoggingReceiver<Input>,
        output_sender: LoggingSender<Output>,
    ) -> Self {
        SimpleMessageBox {
            input_receiver,
            output_sender,
        }
    }

    /// Splits a `SimpleMessageBox` into an input receiver and an output sender,
    /// which can be used to receive and send messages concurrently.
    ///
    /// This method is more efficient than [`into_split`](Self::into_split), but
    /// the halves cannot be moved into independently spawned tasks.
    pub fn split(&mut self) -> (&mut LoggingSender<Output>, &mut LoggingReceiver<Input>) {
        (&mut self.output_sender, &mut self.input_receiver)
    }

    /// Splits a `SimpleMessageBox` into an input receiver and an output sender,
    /// which can be used to receive and send messages concurrently.
    ///
    /// This method returns consumes the `SimpleMessageBox` and returns owned
    /// sender and receiver, which can then be separately moved.
    pub fn into_split(self) -> (LoggingSender<Output>, LoggingReceiver<Input>) {
        (self.output_sender, self.input_receiver)
    }
}

#[async_trait]
impl<Input: Message, Output: Message> MessageReceiver<Input> for SimpleMessageBox<Input, Output> {
    async fn try_recv(&mut self) -> Result<Option<Input>, RuntimeRequest> {
        self.input_receiver.try_recv().await
    }

    async fn recv(&mut self) -> Option<Input> {
        self.input_receiver.recv().await
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.input_receiver.recv_signal().await
    }
}

#[async_trait]
impl<Input: Message, Output: Message> Sender<Output> for SimpleMessageBox<Input, Output> {
    async fn send(&mut self, message: Output) -> Result<(), ChannelError> {
        self.output_sender.send(message).await
    }
}

impl<Input: Message, Output: Message> CloneSender<Output> for SimpleMessageBox<Input, Output> {
    fn sender_clone(&self) -> DynSender<Output> {
        CloneSender::sender_clone(&self.output_sender)
    }

    fn sender(&self) -> Box<dyn Sender<Output>> {
        CloneSender::sender(&self.output_sender)
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

    /// Close the input so no new messages can be sent to this receiver
    pub fn close_input(&mut self) {
        self.input_receiver.close();
        self.signal_receiver.close();
    }
}

#[async_trait]
impl<Input: Send> MessageReceiver<Input> for CombinedReceiver<Input> {
    async fn try_recv(&mut self) -> Result<Option<Input>, RuntimeRequest> {
        tokio::select! {
            biased;

            Some(runtime_request) = self.signal_receiver.next() => {
                Err(runtime_request)
            }
            Some(message) = self.input_receiver.next() => {
                Ok(Some(message))
            }
            else => Ok(None)
        }
    }

    async fn recv(&mut self) -> Option<Input> {
        match self.try_recv().await {
            Ok(Some(message)) => Some(message),
            _ => None,
        }
    }

    async fn recv_signal(&mut self) -> Option<RuntimeRequest> {
        self.signal_receiver.next().await
    }
}
