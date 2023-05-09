use crate::mpsc;
use crate::Actor;
use crate::Builder;
use crate::ClientId;
use crate::ConcurrentServerActor;
use crate::ConcurrentServerMessageBox;
use crate::DynSender;
use crate::KeyedSender;
use crate::LoggingReceiver;
use crate::LoggingSender;
use crate::Message;
use crate::NoConfig;
use crate::RuntimeError;
use crate::RuntimeRequest;
use crate::RuntimeRequestSink;
use crate::Sender;
use crate::SenderVec;
use crate::Server;
use crate::ServerActor;
use crate::ServerMessageBox;
use crate::ServiceProvider;
use crate::SimpleMessageBox;
use std::convert::Infallible;
use std::fmt::Debug;

/// A message box builder for request-response services
pub struct ServerMessageBoxBuilder<Request: Debug, Response> {
    service_name: String,
    max_concurrency: usize,
    request_sender: mpsc::Sender<(ClientId, Request)>,
    input_receiver: LoggingReceiver<(ClientId, Request)>,
    signal_sender: mpsc::Sender<RuntimeRequest>,
    clients: Vec<DynSender<Response>>,
}

impl<Request: Message, Response: Message> ServerMessageBoxBuilder<Request, Response> {
    /// Start to build a new message box for a server
    pub fn new(server_name: &str, capacity: usize) -> Self {
        let max_concurrency = 1;
        let (request_sender, request_receiver) = mpsc::channel(capacity);
        let (signal_sender, signal_receiver) = mpsc::channel(4);
        let input_receiver =
            LoggingReceiver::new(server_name.to_string(), request_receiver, signal_receiver);

        ServerMessageBoxBuilder {
            service_name: server_name.to_string(),
            max_concurrency,
            request_sender,
            input_receiver,
            signal_sender,
            clients: vec![],
        }
    }

    pub fn with_max_concurrency(self, max_concurrency: usize) -> Self {
        Self {
            max_concurrency: std::cmp::max(1, max_concurrency),
            ..self
        }
    }

    /// Build a message box ready to be used by the server actor
    fn build_server(self) -> ServerMessageBox<Request, Response> {
        let response_sender = SenderVec::new_sender(self.clients);
        let logging_sender = LoggingSender::new(self.service_name.clone(), response_sender);

        SimpleMessageBox::new(self.input_receiver, logging_sender)
    }

    /// Build a message box aimed to concurrently serve requests
    fn build_concurrent(self) -> ConcurrentServerMessageBox<Request, Response> {
        let max_concurrency = self.max_concurrency;
        let clients = self.build_server();
        ConcurrentServerMessageBox::new(max_concurrency, clients)
    }
}

impl<Req: Message, Res: Message> RuntimeRequestSink for ServerMessageBoxBuilder<Req, Res> {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.signal_sender.sender_clone()
    }
}

impl<Req: Message, Res: Message> ServiceProvider<Req, Res, NoConfig>
    for ServerMessageBoxBuilder<Req, Res>
{
    fn connect_consumer(
        &mut self,
        _config: NoConfig,
        response_sender: DynSender<Res>,
    ) -> DynSender<Req> {
        let client_id = self.clients.len();
        let request_sender = KeyedSender::new_sender(client_id, self.request_sender.clone());
        self.clients.push(response_sender);
        request_sender
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
        let service_name = server.name().to_string();
        let box_builder = ServerMessageBoxBuilder::new(&service_name, config.capacity)
            .with_max_concurrency(config.max_concurrency);

        ServerActorBuilder {
            _kind: kind,
            server,
            box_builder,
        }
    }
}

impl<S: Server> ServerActorBuilder<S, Sequential> {
    pub async fn run(self) -> Result<(), RuntimeError> {
        let messages = self.box_builder.build();
        let mut actor = ServerActor::new(self.server, messages);

        actor.run().await
    }
}

impl<S: Server + Clone> ServerActorBuilder<S, Concurrent> {
    pub async fn run(self) -> Result<(), RuntimeError> {
        let messages = self.box_builder.build();
        let mut actor = ConcurrentServerActor::new(self.server, messages);

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

impl<S: Server, K> ServiceProvider<S::Request, S::Response, NoConfig> for ServerActorBuilder<S, K> {
    fn connect_consumer(
        &mut self,
        config: NoConfig,
        response_sender: DynSender<S::Response>,
    ) -> DynSender<S::Request> {
        self.box_builder.connect_consumer(config, response_sender)
    }
}

impl<S: Server, K> RuntimeRequestSink for ServerActorBuilder<S, K> {
    fn get_signal_sender(&self) -> DynSender<RuntimeRequest> {
        self.box_builder.get_signal_sender()
    }
}

#[derive(Debug)]
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
