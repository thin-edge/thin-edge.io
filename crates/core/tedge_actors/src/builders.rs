use crate::mpsc;
use crate::ClientId;
use crate::ConcurrentServiceMessageBox;
use crate::DynSender;
use crate::KeyedSender;
use crate::Message;
use crate::NullSender;
use crate::RuntimeError;
use crate::RuntimeHandle;
use crate::Sender;
use crate::SenderVec;
use crate::ServiceMessageBox;
use crate::SimpleMessageBox;
use async_trait::async_trait;
use std::convert::Infallible;

/// Materialize an actor instance under construction
///
/// Such an instance is:
/// 1. built from some actor configuration
/// 2. connected to other peers
/// 3. eventually spawned into an actor.
#[async_trait]
pub trait ActorBuilder {
    /// Build and spawn the actor
    async fn spawn(self, runtime: &mut RuntimeHandle) -> Result<(), RuntimeError>;
}

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
pub struct NoConfig;

/// A trait to connect a message box under-construction to peer messages boxes
pub trait MessageBoxSocket<Request: Message, Response: Message, Config> {
    /// Connect a peer message box to the message box under construction
    fn connect_with(&mut self, peer: &mut impl MessageBoxPlug<Request, Response>, config: Config);
}

/// A connection port to connect a message box under-connection to another box
pub trait MessageBoxPlug<Request: Message, Response: Message> {
    /// Set the sender to be used by this actor's box to send requests
    fn set_request_sender(&mut self, request_sender: DynSender<Request>);

    /// Return a sender where the responses to this actor's box have to be sent
    fn get_response_sender(&self) -> DynSender<Response>;

    /// Connect this client message box to the service message box
    fn connect_to<Config>(
        &mut self,
        service: &mut impl MessageBoxSocket<Request, Response, Config>,
        config: Config,
    ) where
        Self: Sized,
    {
        service.connect_with(self, config)
    }

    /// Connect this client message box to the service message box
    ///
    /// Return the updated client message box.
    fn connected_to<Config>(
        mut self,
        service: &mut impl MessageBoxSocket<Request, Response, Config>,
        config: Config,
    ) -> Self
    where
        Self: Sized,
    {
        service.connect_with(&mut self, config);
        self
    }
}

/// A builder of SimpleMessageBox
pub struct SimpleMessageBoxBuilder<I, O> {
    name: String,
    input_sender: mpsc::Sender<I>,
    input_receiver: mpsc::Receiver<I>,
    output_sender: DynSender<O>,
}

impl<I: Message, O: Message> SimpleMessageBoxBuilder<I, O> {
    pub fn new(name: &str, capacity: usize) -> Self {
        let (input_sender, input_receiver) = mpsc::channel(capacity);
        let output_sender = NullSender.into();
        SimpleMessageBoxBuilder {
            name: name.to_string(),
            input_sender,
            input_receiver,
            output_sender,
        }
    }
}

impl<Req: Message, Res: Message> MessageBoxSocket<Req, Res, NoConfig>
    for SimpleMessageBoxBuilder<Req, Res>
{
    fn connect_with(&mut self, peer: &mut impl MessageBoxPlug<Req, Res>, _config: NoConfig) {
        self.output_sender = peer.get_response_sender();
        peer.set_request_sender(self.input_sender.sender_clone());
    }
}

impl<Req: Message, Res: Message> MessageBoxPlug<Req, Res> for SimpleMessageBoxBuilder<Res, Req> {
    fn set_request_sender(&mut self, output_sender: DynSender<Req>) {
        self.output_sender = output_sender;
    }

    fn get_response_sender(&self) -> DynSender<Res> {
        self.input_sender.sender_clone()
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
        SimpleMessageBox::new(self.name, self.input_receiver, self.output_sender)
    }
}

/// A message box builder for request-response services
pub struct ServiceMessageBoxBuilder<Request, Response> {
    service_name: String,
    max_concurrency: usize,
    request_sender: mpsc::Sender<(ClientId, Request)>,
    request_receiver: mpsc::Receiver<(ClientId, Request)>,
    clients: Vec<DynSender<Response>>,
}

impl<Request: Message, Response: Message> ServiceMessageBoxBuilder<Request, Response> {
    /// Start to build a new message box for a service
    pub fn new(service_name: &str, capacity: usize) -> Self {
        let max_concurrency = 1;
        let (request_sender, request_receiver) = mpsc::channel(capacity);
        ServiceMessageBoxBuilder {
            service_name: service_name.to_string(),
            max_concurrency,
            request_sender,
            request_receiver,
            clients: vec![],
        }
    }

    pub fn with_max_concurrency(self, max_concurrency: usize) -> Self {
        Self {
            max_concurrency: std::cmp::max(1, max_concurrency),
            ..self
        }
    }

    /// Build a message box ready to be used by the service actor
    fn build_service(self) -> ServiceMessageBox<Request, Response> {
        let request_receiver = self.request_receiver;
        let response_sender = SenderVec::new_sender(self.clients);

        SimpleMessageBox::new(self.service_name, request_receiver, response_sender)
    }

    /// Build a message box aimed to concurrently serve requests
    fn build_concurrent(self) -> ConcurrentServiceMessageBox<Request, Response> {
        let max_concurrency = self.max_concurrency;
        let clients = self.build_service();
        ConcurrentServiceMessageBox::new(max_concurrency, clients)
    }
}

impl<Req: Message, Res: Message> MessageBoxSocket<Req, Res, NoConfig>
    for ServiceMessageBoxBuilder<Req, Res>
{
    fn connect_with(&mut self, peer: &mut impl MessageBoxPlug<Req, Res>, _config: NoConfig) {
        let client_id = self.clients.len();
        let request_sender = KeyedSender::new_sender(client_id, self.request_sender.clone());

        self.clients.push(peer.get_response_sender());
        peer.set_request_sender(request_sender)
    }
}

impl<Req: Message, Res: Message> Builder<ServiceMessageBox<Req, Res>>
    for ServiceMessageBoxBuilder<Req, Res>
{
    type Error = Infallible;

    fn try_build(self) -> Result<ServiceMessageBox<Req, Res>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> ServiceMessageBox<Req, Res> {
        self.build_service()
    }
}

impl<Req: Message, Res: Message> Builder<ConcurrentServiceMessageBox<Req, Res>>
    for ServiceMessageBoxBuilder<Req, Res>
{
    type Error = Infallible;

    fn try_build(self) -> Result<ConcurrentServiceMessageBox<Req, Res>, Self::Error> {
        Ok(self.build())
    }

    fn build(self) -> ConcurrentServiceMessageBox<Req, Res> {
        self.build_concurrent()
    }
}
