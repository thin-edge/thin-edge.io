use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use async_trait::async_trait;
use tedge_actors::fan_in_message_type;
use tedge_actors::mpsc;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::StreamExt;
use tedge_http_ext::HttpHandle;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResult;

struct C8YHttpProxyActor {}

#[async_trait]
impl Actor for C8YHttpProxyActor {
    type MessageBox = C8YHttpProxyMessageBox;

    async fn run(self, messages: Self::MessageBox) -> Result<(), ChannelError> {
        C8YHttpProxyActor::run(self, messages).await
    }
}

struct C8YHttpProxyMessageBox {
    /// Requests received by this actor from its clients
    requests: mpsc::Receiver<C8YRestRequest>,

    /// Responses sent by this actor to its clients
    responses: DynSender<C8YRestResponse>,

    /// Handle on some HTTP connection
    http: HttpHandle,
}

fan_in_message_type!(C8YHttpProxyInput[C8YRestRequest, HttpResult] : Debug);
fan_in_message_type!(C8YHttpProxyOutput[C8YRestResponse, HttpRequest] : Debug);

#[async_trait]
impl MessageBox for C8YHttpProxyMessageBox {
    type Input = C8YHttpProxyInput;
    type Output = C8YHttpProxyOutput;

    async fn recv(&mut self) -> Option<Self::Input> {
        tokio::select! {
            Some(message) = self.requests.next() => {
                Some(C8YHttpProxyInput::C8YRestRequest(message))
            },
            Some(message) = self.http.recv() => {
                Some(C8YHttpProxyInput::HttpResult(message))
            },
            else => None,
        }
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        match message {
            C8YHttpProxyOutput::C8YRestResponse(message) => self.responses.send(message).await,
            C8YHttpProxyOutput::HttpRequest(message) => self.http.send(message).await,
        }
    }

    fn new_box(
        _capacity: usize,
        _output: DynSender<Self::Output>,
    ) -> (DynSender<Self::Input>, Self) {
        todo!()
        // Similar impl as for ConfigManagerMessageBox
    }
}

impl C8YHttpProxyActor {
    pub async fn run(self, mut messages: C8YHttpProxyMessageBox) -> Result<(), ChannelError> {
        while let Some(request) = messages.requests.next().await {
            match request {
                C8YRestRequest::C8yCreateEvent(_) => {
                    let request = HttpRequestBuilder::get("http://foo.com")
                        .build()
                        .expect("TODO handle actor specific error");
                    let _response = messages.http.await_response(request).await?;
                    messages.responses.send(().into()).await?;
                }
                C8YRestRequest::C8yUpdateSoftwareListResponse(_) => {}
                C8YRestRequest::UploadLogBinary(_) => {}
                C8YRestRequest::UploadConfigFile(_) => {}
            }
        }
        Ok(())
    }
}
