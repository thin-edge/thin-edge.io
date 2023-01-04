use crate::HttpConnectionBuilder;
use crate::HttpRequest;
use crate::HttpResult;
use async_trait::async_trait;
use tedge_actors::mpsc;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::StreamExt;

pub struct HttpHandle {
    request_sender: DynSender<HttpRequest>,
    response_receiver: mpsc::Receiver<HttpResult>,
}

impl HttpHandle {
    pub(crate) fn new(http: &mut (impl HttpConnectionBuilder + ?Sized)) -> HttpHandle {
        // At most one response is expected
        let (response_sender, response_receiver) = mpsc::channel(1);

        let request_sender = http.connect(response_sender.into());
        HttpHandle {
            request_sender,
            response_receiver,
        }
    }

    pub async fn await_response(
        &mut self,
        request: HttpRequest,
    ) -> Result<HttpResult, ChannelError> {
        self.request_sender.send(request).await?;
        self.response_receiver
            .next()
            .await
            .ok_or(ChannelError::ReceiveError())
    }
}

#[async_trait]
impl MessageBox for HttpHandle {
    type Input = HttpResult;
    type Output = HttpRequest;

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
            HttpHandle {
                request_sender,
                response_receiver,
            },
        )
    }
}
