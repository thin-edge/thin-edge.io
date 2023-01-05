use crate::ChannelError;
use crate::DynSender;
use crate::KeyedSender;
use crate::Message;
use crate::RequestResponseHandler;
use crate::SenderVec;
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::StreamExt;

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
    // TODO add methods to turn on/off logging of input and output messages

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
    /// `let (input_sender, message_box) = MessageBoxImpl::new_box(capacity, output_sender)`
    /// creates a message_box that sends all output messages to the given `output_sender`
    /// and that consumes all the messages sent on the `input_sender` returned along the box.
    fn new_box(capacity: usize, output: DynSender<Self::Output>) -> (DynSender<Self::Input>, Self);
}

/// The basic message box
pub struct SimpleMessageBox<Input, Output> {
    input_receiver: mpsc::Receiver<Input>,
    output_sender: DynSender<Output>,
}

#[async_trait]
impl<Input: Message, Output: Message> MessageBox for SimpleMessageBox<Input, Output> {
    type Input = Input;
    type Output = Output;

    async fn recv(&mut self) -> Option<Input> {
        self.input_receiver.next().await
    }

    async fn send(&mut self, message: Output) -> Result<(), ChannelError> {
        self.output_sender.send(message).await
    }

    fn new_box(capacity: usize, output_sender: DynSender<Output>) -> (DynSender<Input>, Self) {
        let (input_sender, input_receiver) = mpsc::channel(capacity);
        let message_box = SimpleMessageBox {
            input_receiver,
            output_sender,
        };
        (input_sender.into(), message_box)
    }
}

/// A message box for a request-response service
pub struct ServiceMessageBox<Request, Response> {
    /// Requests received by this actor from its clients
    requests: mpsc::Receiver<(ClientId, Request)>,

    /// Responses sent by this actor to its clients
    responses: DynSender<(ClientId, Response)>,
}

type ClientId = usize;

#[async_trait]
impl<Request: Message, Response: Message> MessageBox for ServiceMessageBox<Request, Response> {
    type Input = (ClientId, Request);
    type Output = (ClientId, Response);

    async fn recv(&mut self) -> Option<Self::Input> {
        self.requests.next().await
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        self.responses.send(message).await
    }

    fn new_box(capacity: usize, output: DynSender<Self::Output>) -> (DynSender<Self::Input>, Self) {
        let (request_sender, input) = mpsc::channel(capacity);
        let message_box = ServiceMessageBox {
            requests: input,
            responses: output,
        };
        (request_sender.into(), message_box)
    }
}

/// A message box builder for request-response service
pub struct ServiceMessageBoxBuilder<Request, Response> {
    request_sender: mpsc::Sender<(ClientId, Request)>,
    request_receiver: mpsc::Receiver<(ClientId, Request)>,
    clients: Vec<DynSender<Response>>,
}

impl<Request: Message, Response: Message> ServiceMessageBoxBuilder<Request, Response> {
    /// Start to build a new message box for a service
    pub fn new(capacity: usize) -> Self {
        let (request_sender, request_receiver) = mpsc::channel(capacity);
        ServiceMessageBoxBuilder {
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
    pub fn add_client(&mut self) -> RequestResponseHandler<Request, Response> {
        // At most one response is expected
        let (response_sender, response_receiver) = mpsc::channel(1);

        let request_sender = self.connect(response_sender.into());
        RequestResponseHandler {
            request_sender,
            response_receiver,
        }
    }

    /// Build a message box ready to be used by the service actor
    pub fn build(self) -> ServiceMessageBox<Request, Response> {
        let request_receiver = self.request_receiver;
        let response_sender = SenderVec::new_sender(self.clients);

        ServiceMessageBox {
            requests: request_receiver,
            responses: response_sender,
        }
    }
}
