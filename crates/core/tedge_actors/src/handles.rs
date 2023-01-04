use crate::mpsc;
use crate::ChannelError;
use crate::DynSender;
use crate::Message;
use crate::MessageBox;
use crate::StreamExt;
use async_trait::async_trait;

/// Client side handler of requests/responses sent to an actor
pub struct RequestResponseHandler<Request, Response> {
    pub(crate) request_sender: DynSender<Request>,
    pub(crate) response_receiver: mpsc::Receiver<Response>,
}

impl<Request: Message, Response: Message> RequestResponseHandler<Request, Response> {
    /// Send the request and await for a response
    pub async fn await_response(&mut self, request: Request) -> Result<Response, ChannelError> {
        self.request_sender.send(request).await?;
        self.response_receiver
            .next()
            .await
            .ok_or(ChannelError::ReceiveError())
    }
}

#[async_trait]
impl<Request: Message, Response: Message> MessageBox for RequestResponseHandler<Request, Response> {
    type Input = Response;
    type Output = Request;

    async fn recv(&mut self) -> Option<Self::Input> {
        self.response_receiver.next().await
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        self.request_sender.send(message).await
    }

    fn new_box(
        _capacity: usize,
        request_sender: DynSender<Self::Output>,
    ) -> (DynSender<Self::Input>, Self) {
        let (response_sender, response_receiver) = mpsc::channel(1);
        (
            response_sender.into(),
            Self {
                request_sender,
                response_receiver,
            },
        )
    }
}
