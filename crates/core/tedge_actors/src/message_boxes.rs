use crate::ChannelError;
use crate::DynSender;
use crate::KeyedSender;
use crate::Message;
use crate::RequestResponseHandler;
use crate::SenderVec;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::StreamExt;
use log::debug;
use log::info;

/// A message box used by an actor to collect all its input and forward its output
///
/// This message box can be seen as two streams of messages,
/// - inputs sent to the actor and stored in the message box awaiting to be processed,
/// - outputs sent by the actor and forwarded to the message box of other actors.
///
/// ```logical-view
/// input_sender: DynSender<Input> -----> |||||| Box ----> output_sender: DynSender<Input>
/// ```
///
/// Under the hood, a `MessageBox` implementation can use
/// - several input channels to await messages from specific peers
///   .e.g. awaiting a response from an HTTP actor
///    and ignoring the kind of events till a response or a timeout has been received.
/// - several output channels to send messages to specific peers.
/// - provide helper function that combine internal channels.
#[async_trait]
pub trait MessageBox: 'static + Sized + Send + Sync {
    /// Type of input messages the actor consumes
    type Input: Message;

    /// Type of output messages the actor produces
    type Output: Message;

    /// Return the next available input message if any
    ///
    /// Await for a message if there is not message yet.
    /// Return `None` if no more message can be received because all the senders have been dropped.
    async fn recv(&mut self) -> Option<Self::Input>;

    /// Send an output message.
    ///
    /// Fail if there is no more receiver expecting these messages.
    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError>;

    /// Crate a message box
    ///
    /// `let (input_sender, message_box) = MessageBoxImpl::new_box(name, capacity, output_sender)`
    /// creates a message_box that sends all output messages to the given `output_sender`
    /// and that consumes all the messages sent on the `input_sender` returned along the box.
    fn new_box(
        name: &str,
        capacity: usize,
        output: DynSender<Self::Output>,
    ) -> (DynSender<Self::Input>, Self);

    /// Turn on/off logging of input and output messages
    fn turn_logging_on(&mut self, on: bool);

    /// Name of the associated actor
    fn name(&self) -> &str;

    /// Log an input message just after reception, before processing it.
    fn log_input(&self, message: &Self::Input) {
        if self.logging_is_on() {
            info!(target: self.name(), "recv {:?}", message);
        }
    }

    /// Log an output message just before sending it.
    fn log_output(&self, message: &Self::Output) {
        if self.logging_is_on() {
            debug!(target: self.name(), "send {:?}", message);
        }
    }

    fn logging_is_on(&self) -> bool;
}

/// The basic message box
pub struct SimpleMessageBox<Input, Output> {
    name: String,
    input_receiver: mpsc::Receiver<Input>,
    output_sender: DynSender<Output>,
    logging_is_on: bool,
}

impl<Input: Message, Output: Message> SimpleMessageBox<Input, Output> {
    pub(crate) fn new(
        name: String,
        input_receiver: mpsc::Receiver<Input>,
        output_sender: DynSender<Output>,
    ) -> Self {
        SimpleMessageBox {
            name,
            input_receiver,
            output_sender,
            logging_is_on: true,
        }
    }
}

#[async_trait]
impl<Input: Message, Output: Message> MessageBox for SimpleMessageBox<Input, Output> {
    type Input = Input;
    type Output = Output;

    async fn recv(&mut self) -> Option<Input> {
        self.input_receiver.next().await.map(|message| {
            self.log_input(&message);
            message
        })
    }

    async fn send(&mut self, message: Output) -> Result<(), ChannelError> {
        self.log_output(&message);
        self.output_sender.send(message).await
    }

    fn new_box(
        name: &str,
        capacity: usize,
        output_sender: DynSender<Output>,
    ) -> (DynSender<Input>, Self) {
        let (input_sender, input_receiver) = mpsc::channel(capacity);
        let message_box = SimpleMessageBox::new(name.to_string(), input_receiver, output_sender);
        (input_sender.into(), message_box)
    }

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

/// A message box for a request-response service
pub type ServiceMessageBox<Request, Response> =
    SimpleMessageBox<(ClientId, Request), (ClientId, Response)>;

type ClientId = usize;

/// A message box builder for request-response service
pub struct ServiceMessageBoxBuilder<Request, Response> {
    service_name: String,
    request_sender: mpsc::Sender<(ClientId, Request)>,
    request_receiver: mpsc::Receiver<(ClientId, Request)>,
    clients: Vec<DynSender<Response>>,
}

impl<Request: Message, Response: Message> ServiceMessageBoxBuilder<Request, Response> {
    /// Start to build a new message box for a service
    pub fn new(service_name: &str, capacity: usize) -> Self {
        let (request_sender, request_receiver) = mpsc::channel(capacity);
        ServiceMessageBoxBuilder {
            service_name: service_name.to_string(),
            request_sender,
            request_receiver,
            clients: vec![],
        }
    }

    /// Connect a new client that expects responses on the provided channel
    ///
    /// Return a channel to which requests will have to be sent.
    pub fn connect(&mut self, client: DynSender<Response>) -> DynSender<Request> {
        let client_id = self.clients.len();
        self.clients.push(client);

        KeyedSender::new_sender(client_id, self.request_sender.clone())
    }

    /// Add a new client, returning a message box to send requests and awaiting responses
    pub fn add_client(&mut self, client_name: &str) -> RequestResponseHandler<Request, Response> {
        // At most one response is expected
        let (response_sender, response_receiver) = mpsc::channel(1);

        let request_sender = self.connect(response_sender.into());
        RequestResponseHandler::new(
            &format!("{} -> {}", client_name, self.service_name),
            response_receiver,
            request_sender,
        )
    }

    /// Build a message box ready to be used by the service actor
    pub fn build(self) -> ServiceMessageBox<Request, Response> {
        let request_receiver = self.request_receiver;
        let response_sender = SenderVec::new_sender(self.clients);

        SimpleMessageBox {
            input_receiver: request_receiver,
            output_sender: response_sender,
            name: self.service_name,
            logging_is_on: true,
        }
    }
}
