use crate::c8y_http_proxy::messages::{C8YRestRequest, C8YRestResponse};
use async_trait::async_trait;
use tedge_actors::{mpsc, Actor, ChannelError, DynSender, MessageBox, StreamExt};
use tedge_http_ext::{HttpRequest, HttpResult};

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

    /// Requests sent by this actor over HTTP
    http_requests: DynSender<HttpRequest>,

    /// Responses received by this actor over HTTP
    http_responses: mpsc::Receiver<HttpResult>,
}

impl C8YHttpProxyMessageBox {
    pub async fn send_http_request(
        &mut self,
        request: HttpRequest,
    ) -> Result<HttpResult, ChannelError> {
        self.http_requests.send(request).await?;
        self.http_responses
            .next()
            .await
            .ok_or(ChannelError::ReceiveError())
    }
}

impl MessageBox for C8YHttpProxyMessageBox {}

impl C8YHttpProxyActor {
    pub async fn run(self, mut messages: C8YHttpProxyMessageBox) -> Result<(), ChannelError> {
        while let Some(request) = messages.requests.next().await {
            match request {
                C8YRestRequest::C8yCreateEvent(_) => {
                    let request = HttpRequest::new(Default::default(), "")
                        .expect("TODO handle actor specific error");
                    let _response = messages.send_http_request(request).await?;
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
