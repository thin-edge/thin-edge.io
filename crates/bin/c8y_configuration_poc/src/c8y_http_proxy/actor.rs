use crate::c8y_http_proxy::credentials::JwtRequest;
use crate::c8y_http_proxy::credentials::JwtResult;
use crate::c8y_http_proxy::credentials::JwtRetriever;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use async_trait::async_trait;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::DynSender;
use tedge_actors::MessageBox;
use tedge_actors::ServiceMessageBox;
use tedge_http_ext::HttpHandle;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResult;

pub(crate) struct C8YHttpProxyActor {}

#[async_trait]
impl Actor for C8YHttpProxyActor {
    type MessageBox = C8YHttpProxyMessageBox;

    async fn run(self, messages: Self::MessageBox) -> Result<(), ChannelError> {
        C8YHttpProxyActor::run(self, messages).await
    }
}

pub(crate) struct C8YHttpProxyMessageBox {
    /// Connection to the clients
    pub(crate) clients: ServiceMessageBox<C8YRestRequest, C8YRestResponse>,

    /// Connection to an HTTP actor
    pub(crate) http: HttpHandle,

    /// Connection to a JWT token retriever
    pub(crate) jwt: JwtRetriever,
}

#[derive(Debug)]
pub struct C8YRestRequestWithClientId(usize, C8YRestRequest);

#[derive(Debug)]
pub struct C8YRestResponseWithClientId(usize, C8YRestResponse);

fan_in_message_type!(C8YHttpProxyInput[C8YRestRequestWithClientId, HttpResult, JwtResult] : Debug);
fan_in_message_type!(C8YHttpProxyOutput[C8YRestResponseWithClientId, HttpRequest, JwtRequest] : Debug);

// TODO: Can such a MessageBox implementation be derived from a struct of message boxes?
#[async_trait]
impl MessageBox for C8YHttpProxyMessageBox {
    type Input = C8YHttpProxyInput;
    type Output = C8YHttpProxyOutput;

    async fn recv(&mut self) -> Option<Self::Input> {
        tokio::select! {
            Some((id,message)) = self.clients.recv() => {
                Some(C8YHttpProxyInput::C8YRestRequestWithClientId(C8YRestRequestWithClientId (id, message)))
            },
            Some(message) = self.http.recv() => {
                Some(C8YHttpProxyInput::HttpResult(message))
            },
            Some(message) = self.jwt.recv() => {
                Some(C8YHttpProxyInput::JwtResult(message))
            },
            else => None,
        }
    }

    async fn send(&mut self, message: Self::Output) -> Result<(), ChannelError> {
        match message {
            C8YHttpProxyOutput::C8YRestResponseWithClientId(message) => {
                self.clients.send((message.0, message.1)).await
            }
            C8YHttpProxyOutput::HttpRequest(message) => self.http.send(message).await,
            C8YHttpProxyOutput::JwtRequest(message) => self.jwt.send(message).await,
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
        while let Some((client_id, request)) = messages.clients.recv().await {
            match request {
                C8YRestRequest::C8yCreateEvent(_) => {
                    let request = HttpRequestBuilder::get("http://foo.com")
                        .build()
                        .expect("TODO handle actor specific error");
                    let _response = messages.http.await_response(request).await?;
                    messages.clients.send((client_id, ().into())).await?;
                }
                C8YRestRequest::C8yUpdateSoftwareListResponse(_) => {}
                C8YRestRequest::UploadLogBinary(_) => {}
                C8YRestRequest::UploadConfigFile(_) => {}
            }
        }
        Ok(())
    }
}
