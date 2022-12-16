use crate::{ChannelError, DynSender, Message};
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
