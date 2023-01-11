use crate::mpsc;
use crate::ChannelError;
use crate::DynSender;
use crate::Message;
use crate::MessageBox;
use crate::SimpleMessageBox;
use async_trait::async_trait;

/// Client side handler of requests/responses sent to an actor
///
/// TODO since this is a MessageBox for a client of a service,
///      a better name could ClientMessageBox.
pub struct RequestResponseHandler<Request, Response> {
    // Note that this message box sends requests and receive responses.
    messages: SimpleMessageBox<Response, Request>,
}

impl<Request: Message, Response: Message> RequestResponseHandler<Request, Response> {
    pub(crate) fn new(
        name: &str,
        response_receiver: mpsc::Receiver<Response>,
        request_sender: DynSender<Request>,
    ) -> Self {
        RequestResponseHandler {
            messages: SimpleMessageBox::new(name.to_string(), response_receiver, request_sender),
        }
    }

    /// Send the request and await for a response
    pub async fn await_response(&mut self, request: Request) -> Result<Response, ChannelError> {
        self.messages.send(request).await?;
        self.messages
            .recv()
            .await
            .ok_or(ChannelError::ReceiveError())
    }
}

#[async_trait]
impl<Request: Message, Response: Message> MessageBox for RequestResponseHandler<Request, Response> {
    type Input = Response;
    type Output = Request;

    async fn recv(&mut self) -> Option<Self::Input> {
        self.messages.recv().await
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        self.messages.send(message).await
    }

    fn turn_logging_on(&mut self, on: bool) {
        self.messages.turn_logging_on(on)
    }

    fn name(&self) -> &str {
        self.messages.name()
    }

    fn logging_is_on(&self) -> bool {
        self.messages.logging_is_on()
    }
}
