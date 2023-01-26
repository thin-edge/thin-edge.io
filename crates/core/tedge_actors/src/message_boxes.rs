use crate::Builder;
use crate::ChannelError;
use crate::DynSender;
use crate::Message;
use crate::MessageBoxSocket;
use crate::NoConfig;
use crate::SimpleMessageBoxBuilder;
use futures::channel::mpsc;
use futures::StreamExt;
use log::debug;
use log::info;
use std::fmt::Debug;

/// A message box used by an actor to collect all its input and forward its output
///
/// Conceptually, a message box is a `Receiver<Input: Message>`, from where an actor
/// receives input messages, and a `DynSender<Output: Message>`, through which the actor
/// sends output messages. The receiver can be connected to the senders of peer actors;
/// and reciprocally the sender can be connected to several receivers of peer actors.
/// * The receivers are `mpsc::Receiver` that collect messages from several sources.
/// * The senders are `DynSender` that adapt the messages sent to match constraints of the receivers.
///
/// A `SimpleMessageBox<Input, Output>` implements exactly this conceptual view:
///
/// ```ascii
///                    input_senders: DynSender<Input> ...
///
///                                   │
///         ┌─────────────────────────┴───────────────────────────┐
///         │                         ▼                           │
///         │         input_receiver: mpsc::Receiver<Input>       │
///         │                                                     │
///         │                         │                           │
///         │                         │                           │
///         │                         ▼                           │
///         │                    actor: Actor                     │
///         │                                                     │
///         │                         │                           │
///         │                         │                           │
///         │                         ▼                           │
///         │          output_sender: DynSender<Output>           │
///         │                                                     │
///         └─────────────────────────┬───────────────────────────┘
///                                   │
///                                   ▼
///                output_receivers: mpsc::Receiver<Output> ...
/// ```
///
/// However, collecting all the messages in a single receiver prevents
/// the actor to process messages with a different priority according to their sources.
/// So, in practice, actors use specific message boxes to match specific needs.
///
/// Here is a typical message box that
/// - handles not only regular Input and Output messages
/// - but also processes runtime requests with a higher priority
/// - and awaits for responses for HTTP requests.
///
/// ```ascii
///
///                     │                                      │
/// ┌───────────────────┴──────────────────────────────────────┴─────────────────────────┐
/// │                   ▼                                      ▼                         │
/// │   input_receiver: mpsc::Receiver<Input>     runtime: Receiver<RuntimeRequest>      │
/// │                   │                                                                │
/// │                   │                                                                │
/// │                   ▼                         http_request: DynSender<HttpRequest> ──┼────►
/// │              actor: Actor                                                          │
/// │                   │                        http_response: Receiver<HttpResponse> ◄─┼─────
/// │                   │                                                                │
/// │                   ▼                                                                │
/// │    output_sender: DynSender<Output>                                                │
/// │                                                                                    │
/// └───────────────────┬────────────────────────────────────────────────────────────────┘
///                     │
///                     ▼
/// ```
///
/// In order to let peer actors to such a message box with specific channels,
/// the actor implementation must provide a message box builder that implements the following traits:
///
/// - `MessageSource`
/// - `MessageSink`
/// - `MessageBoxSocket`
/// - `MessageBoxPlug`
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

/// The builder of a MessageBox must implement this trait for every message type that can be sent to it
pub trait MessageSink<M: Message> {
    /// Return the sender that can be used by peers to send messages to this actor
    fn get_sender(&self) -> DynSender<M>;
}

/// The builder of a MessageBox must implement this trait for every message type that it can receive from its peers
pub trait MessageSource<M: Message, Config> {
    /// The message will be sent to the peer using the provided `sender`
    fn register_peer(&mut self, config: Config, sender: DynSender<M>);
}

/// The basic message box
pub struct SimpleMessageBox<Input, Output> {
    name: String,
    input_receiver: mpsc::Receiver<Input>,
    output_sender: DynSender<Output>,
    logging_is_on: bool,
}

impl<Input: Message, Output: Message> SimpleMessageBox<Input, Output> {
    pub fn new(
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

    pub async fn recv(&mut self) -> Option<Input> {
        self.input_receiver.next().await.map(|message| {
            self.log_input(&message);
            message
        })
    }

    pub async fn send(&mut self, message: Output) -> Result<(), ChannelError> {
        self.log_output(&message);
        self.output_sender.send(message).await
    }

    /// Create a message box pair (mostly for testing purpose)
    ///
    /// - The first message box is used to control and observe the second box.
    /// - Messages sent from the first message box are received by the second box.
    /// - Messages sent from the second message box are received by the first box.
    /// - The first message box is always a SimpleMessageBox.
    /// - The second message box is of the specific message box type expected by the actor under test.
    pub fn channel(name: &str, capacity: usize) -> (SimpleMessageBox<Output, Input>, Self) {
        let mut client_box = SimpleMessageBoxBuilder::new(&format!("{}-Client", name), capacity);
        let mut service_box = SimpleMessageBoxBuilder::new(&format!("{}-Service", name), capacity);
        service_box.connect_with(&mut client_box, NoConfig);
        (client_box.build(), service_box.build())
    }

    /// Close the sending channel of this message box.
    ///
    /// This makes the receiving end aware that no more message will be sent.
    pub fn close_output(&mut self) {
        self.output_sender.close_sender()
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

/// A message box for a request-response service
pub type ServiceMessageBox<Request, Response> =
    SimpleMessageBox<(ClientId, Request), (ClientId, Response)>;

pub type ClientId = usize;

/// A message box for services that handles requests concurrently
pub struct ConcurrentServiceMessageBox<Request, Response> {
    /// Max concurrent requests
    max_concurrency: usize,

    /// Message box to interact with clients of this service
    clients: ServiceMessageBox<Request, Response>,

    /// Pending responses
    pending_responses: futures::stream::FuturesUnordered<PendingResult<(usize, Response)>>,
}

type PendingResult<R> = tokio::task::JoinHandle<R>;

type RawClientMessageBox<Request, Response> =
    SimpleMessageBox<(ClientId, Response), (ClientId, Request)>;

impl<Request: Message, Response: Message> ConcurrentServiceMessageBox<Request, Response> {
    pub(crate) fn new(
        max_concurrency: usize,
        clients: ServiceMessageBox<Request, Response>,
    ) -> Self {
        ConcurrentServiceMessageBox {
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

    pub fn channel(
        name: &str,
        capacity: usize,
        max_concurrency: usize,
    ) -> (RawClientMessageBox<Request, Response>, Self) {
        let (client_box, service_box) = SimpleMessageBox::channel(name, capacity);
        let concurrent_service_box = ConcurrentServiceMessageBox::new(max_concurrency, service_box);
        (client_box, concurrent_service_box)
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
    for ConcurrentServiceMessageBox<Request, Response>
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
