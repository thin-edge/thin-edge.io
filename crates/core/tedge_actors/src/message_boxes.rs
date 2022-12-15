use crate::{ChannelError, DynSender, LinkError, Message};
use async_trait::async_trait;
use futures::channel::mpsc;
use futures::StreamExt;

/// A message box used by an actor to collect all its input and forward its output
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
}

/// The basic message box builder
///
/// TODO: remove this builder that is not useful
pub struct SimpleMessageBoxBuilder<Input, Output> {
    pub sender: mpsc::Sender<Input>,
    pub input: mpsc::Receiver<Input>,
    pub output: Option<DynSender<Output>>,
}

impl<Input: Message, Output: Message> SimpleMessageBoxBuilder<Input, Output> {
    pub fn new(size: usize) -> Self {
        let (sender, input) = mpsc::channel(size);
        SimpleMessageBoxBuilder {
            sender,
            input,
            output: None,
        }
    }

    pub fn build(self) -> Result<SimpleMessageBox<Input, Output>, LinkError> {
        if let Some(output) = self.output {
            Ok(SimpleMessageBox {
                input: self.input,
                output,
            })
        } else {
            Err(LinkError::MissingPeer {
                role: "output".to_string(),
            })
        }
    }

    pub fn get_input(&self) -> DynSender<Input> {
        self.sender.clone().into()
    }

    pub fn set_output(&mut self, output: DynSender<Output>) -> Result<(), LinkError> {
        if self.output.is_some() {
            Err(LinkError::ExcessPeer {
                role: "output".to_string(),
            })
        } else {
            self.output = Some(output);
            Ok(())
        }
    }
}

/// The basic message box
pub struct SimpleMessageBox<Input, Output> {
    input: mpsc::Receiver<Input>,
    output: DynSender<Output>,
}

#[async_trait]
impl<Input: Message, Output: Message> MessageBox for SimpleMessageBox<Input, Output> {
    type Input = Input;
    type Output = Output;

    // TODO log each message before returning it
    async fn recv(&mut self) -> Option<Input> {
        self.input.next().await
    }

    // TODO log each message before sending it
    async fn send(&mut self, message: Output) -> Result<(), ChannelError> {
        self.output.send(message).await
    }
}
