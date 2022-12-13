use crate::{ChannelError, DynSender, LinkError, Message};
use futures::channel::mpsc;
use futures::StreamExt;

/// A message box used by an actor to collect all its input and forward its output
pub trait MessageBox: 'static + Sized + Send + Sync {
    // TODO add methods to turn on/off logging of input and output messages
}

/// Build a message box adapted to an actor
pub trait MessageBoxBuilder {
    /// Type of input messages the actor consumes
    type Input: Message;

    /// Type of output messages the actor produces
    type Output: Message;

    /// The type of box built by this builder
    /// used by an actor to receive and send messages
    ///
    /// This box might depend only indirectly on the Input and Output types.
    /// This is notably the case when the box distinguish specific kind of inputs or outputs.
    type MessageBox: MessageBox;

    /// Build the message box
    ///
    /// Return an error if no output has been set
    fn build(self) -> Result<Self::MessageBox, LinkError>;

    /// Get an input sender to the box under construction
    ///
    /// In practice, a specific builder will also provide fine-grain getters
    /// to get senders for specific sub-types of input messages
    fn get_input(&self) -> DynSender<Self::Input>;

    /// Set the output sender of the box under construction
    ///
    /// In practice, a specific builder will also provide fine-grain setters
    /// to assign senders for a specific sub-types of output messages.
    fn set_output(&mut self, output: DynSender<Self::Output>) -> Result<(), LinkError>;
}

/// The basic message box builder
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
}

impl<Input: Message, Output: Message> MessageBoxBuilder for SimpleMessageBoxBuilder<Input, Output> {
    type Input = Input;
    type Output = Output;
    type MessageBox = SimpleMessageBox<Input, Output>;

    fn build(self) -> Result<Self::MessageBox, LinkError> {
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

    fn get_input(&self) -> DynSender<Input> {
        self.sender.clone().into()
    }

    fn set_output(&mut self, output: DynSender<Output>) -> Result<(), LinkError> {
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

impl<Input: Message, Output: Message> MessageBox for SimpleMessageBox<Input, Output> {}

impl<Input: Message, Output: Message> SimpleMessageBox<Input, Output> {
    // TODO log each message before returning it
    pub async fn next(&mut self) -> Option<Input> {
        self.input.next().await
    }

    // TODO log each message before sending it
    pub async fn send(&mut self, message: Output) -> Result<(), ChannelError> {
        self.output.send(message).await
    }
}
