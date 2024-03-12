use crate::mpsc;
use crate::Actor;
use crate::Builder;
use crate::CloneSender;
use crate::ConcurrentServerActor;
use crate::ConcurrentServerMessageBox;
use crate::DynRequestSender;
use crate::DynSender;
use crate::LoggingReceiver;
use crate::Message;
use crate::MessageSink;
use crate::RequestEnvelope;
use crate::RuntimeError;
use crate::RuntimeRequest;
use crate::RuntimeRequestSink;
use crate::Server;
use crate::ServerActor;
use crate::ServerMessageBox;
use std::convert::Infallible;
use std::fmt::Debug;

/// A message box builder for request-response services
pub struct ServerMessageBoxBuilder<Request: Debug, Response> {
    max_concurrency: usize,
    request_sender: mpsc::Sender<RequestEnvelope<Request, Response>>,
    request_receiver: LoggingReceiver<RequestEnvelope<Request, Response>>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
}

impl<Request: Message, Response: Message> ServerMessageBoxBuilder<Request, Response> {
    /// Start to build a new message box for a server
    pub fn new(server_name: &str, capacity: usize) -> Self {
        let max_concurrency = 1;
        let (request_sender, request_receiver) = mpsc::channel(capacity);
        let (signal_sender, signal_receiver) = mpsc::channel(4);
        let request_receiver =
            LoggingReceiver::new(server_name.to_string(), request_receiver, signal_receiver);

        ServerMessageBoxBuilder {
            max_concurrency,
            request_sender,
            request_receiver,
            signal_sender,
        }
    }

    pub fn with_max_concurrency(self, max_concurrency: usize) -> Self {
        Self {
            max_concurrency: std::cmp::max(1, max_concurrency),
            ..self
        }
    }

    /// Return a sender for the requests
    pub fn request_sender(&self) -> DynRequestSender<Request, Response> {
        self.request_sender.sender_clone()
    }

    /// Build a message box ready to be used by the server actor
    fn build_server(self) -> ServerMessageBox<Request, Response> {
        self.request_receiver
    }

    /// Build a message box aimed to concurrently serve requests
    fn build_concurrent(self) -> ConcurrentServerMessageBox<Request, Response> {
        ConcurrentServerMessageBox::new(self.max_concurrency, self.request_receiver)
    }
}

impl<Req: Message, Res: Message> RuntimeRequestSink for ServerMessageBoxBuilder<Req, Res> {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.signal_sender.sender_clone()
    }
}

impl<Req: Message, Res: Message> MessageSink<RequestEnvelope<Req, Res>>
    for ServerMessageBoxBuilder<Req, Res>
{
    fn get_sender(&self) -> DynSender<RequestEnvelope<Req, Res>> {
        self.request_sender().sender_clone()
    }
}

impl<Req: Message, Res: Message> Builder<ServerMessageBox<Req, Res>>
    for ServerMessageBoxBuilder<Req, Res>
{
    type Error = Infallible;

    fn try_build(self) -> Result<ServerMessageBox<Req, Res>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> ServerMessageBox<Req, Res> {
        self.build_server()
    }
}

impl<Req: Message, Res: Message> Builder<ConcurrentServerMessageBox<Req, Res>>
    for ServerMessageBoxBuilder<Req, Res>
{
    type Error = Infallible;

    fn try_build(self) -> Result<ConcurrentServerMessageBox<Req, Res>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> ConcurrentServerMessageBox<Req, Res> {
        self.build_concurrent()
    }
}

pub struct Concurrent;
pub struct Sequential;

/// A Server Actor builder
///
/// The type K is the kind of concurrency: Sequential or Concurrent
pub struct ServerActorBuilder<S: Server, K> {
    _kind: K,
    server: S,
    box_builder: ServerMessageBoxBuilder<S::Request, S::Response>,
}

impl<S: Server, K> ServerActorBuilder<S, K> {
    pub fn new(server: S, config: &ServerConfig, kind: K) -> Self {
        let box_builder = ServerMessageBoxBuilder::new(server.name(), config.capacity)
            .with_max_concurrency(config.max_concurrency);

        ServerActorBuilder {
            _kind: kind,
            server,
            box_builder,
        }
    }

    /// Return a sender for the requests
    pub fn request_sender(&self) -> DynRequestSender<S::Request, S::Response> {
        self.box_builder.request_sender()
    }
}

impl<S: Server> ServerActorBuilder<S, Sequential> {
    pub async fn run(self) -> Result<(), RuntimeError> {
        let messages = self.box_builder.build();
        let actor = ServerActor::new(self.server, messages);

        actor.run().await
    }
}

impl<S: Server + Clone> ServerActorBuilder<S, Concurrent> {
    pub async fn run(self) -> Result<(), RuntimeError> {
        let messages = self.box_builder.build();
        let actor = ConcurrentServerActor::new(self.server, messages);

        actor.run().await
    }
}

impl<S: Server> Builder<ServerActor<S>> for ServerActorBuilder<S, Sequential> {
    type Error = Infallible;

    fn try_build(self) -> Result<ServerActor<S>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> ServerActor<S> {
        let actor_box = self.box_builder.build();
        ServerActor::new(self.server, actor_box)
    }
}

impl<S: Server + Clone> Builder<ConcurrentServerActor<S>> for ServerActorBuilder<S, Concurrent> {
    type Error = Infallible;

    fn try_build(self) -> Result<ConcurrentServerActor<S>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> ConcurrentServerActor<S> {
        let actor_box = self.box_builder.build();
        ConcurrentServerActor::new(self.server, actor_box)
    }
}

impl<S: Server, K> MessageSink<RequestEnvelope<S::Request, S::Response>>
    for ServerActorBuilder<S, K>
{
    fn get_sender(&self) -> DynSender<RequestEnvelope<S::Request, S::Response>> {
        self.box_builder.get_sender()
    }
}

impl<S: Server, K> RuntimeRequestSink for ServerActorBuilder<S, K> {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ServerConfig {
    pub capacity: usize,
    pub max_concurrency: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            capacity: 16,
            max_concurrency: 4,
        }
    }
}

impl ServerConfig {
    pub fn new() -> Self {
        ServerConfig::default()
    }

    pub fn with_capacity(self, capacity: usize) -> Self {
        Self { capacity, ..self }
    }

    pub fn with_max_concurrency(self, max_concurrency: usize) -> Self {
        Self {
            max_concurrency,
            ..self
        }
    }
}
