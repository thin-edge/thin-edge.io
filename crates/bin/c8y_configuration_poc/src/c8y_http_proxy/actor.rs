use crate::c8y_http_proxy::credentials::JwtRequest;
use crate::c8y_http_proxy::credentials::JwtResult;
use crate::c8y_http_proxy::credentials::JwtRetriever;
use crate::c8y_http_proxy::messages::C8YRestError;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResult;
use crate::c8y_http_proxy::messages::DownloadFile;
use crate::c8y_http_proxy::messages::EventId;
use crate::c8y_http_proxy::messages::Unit;
use crate::c8y_http_proxy::messages::UploadConfigFile;
use crate::c8y_http_proxy::messages::UploadLogBinary;
use async_trait::async_trait;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
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

    fn name(&self) -> &str {
        "C8YHttpProxy"
    }

    async fn run(self, messages: Self::MessageBox) -> Result<(), ChannelError> {
        C8YHttpProxyActor::run(self, messages).await
    }
}

pub(crate) struct C8YHttpProxyMessageBox {
    /// Connection to the clients
    pub(crate) clients: ServiceMessageBox<C8YRestRequest, C8YRestResult>,

    /// Connection to an HTTP actor
    pub(crate) http: HttpHandle,

    /// Connection to a JWT token retriever
    pub(crate) jwt: JwtRetriever,
}

#[derive(Debug)]
pub struct C8YRestRequestWithClientId(usize, C8YRestRequest);

#[derive(Debug)]
pub struct C8YRestResponseWithClientId(usize, C8YRestResult);

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
        // FIXME Is this method useful?
        todo!()
        // Similar impl as for ConfigManagerMessageBox
    }
}

impl C8YHttpProxyActor {
    pub async fn run(mut self, mut messages: C8YHttpProxyMessageBox) -> Result<(), ChannelError> {
        let clients = &mut messages.clients;
        let http = &mut messages.http;
        let jwt = &mut messages.jwt;

        while let Some((client_id, request)) = clients.recv().await {
            let result = match request {
                C8YRestRequest::C8yCreateEvent(request) => self
                    .create_event(http, jwt, request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::C8yUpdateSoftwareListResponse(request) => self
                    .send_software_list_http(http, jwt, request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::UploadLogBinary(request) => self
                    .upload_log_binary(http, jwt, request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::UploadConfigFile(request) => self
                    .upload_config_file(http, jwt, request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::DownloadFile(request) => self
                    .download_file(http, jwt, request)
                    .await
                    .map(|response| response.into()),
            };
            clients.send((client_id, result).into()).await?;
        }
        Ok(())
    }

    async fn create_event(
        &mut self,
        http: &mut HttpHandle,
        _jwt: &mut JwtRetriever,
        _request: C8yCreateEvent,
    ) -> Result<EventId, C8YRestError> {
        let http_request = HttpRequestBuilder::get("http://foo.com")
            .build()
            .expect("TODO handle actor specific error");
        let http_result = http.await_response(http_request).await?;
        Ok("TODO".to_string())
    }

    async fn send_software_list_http(
        &mut self,
        _http: &mut HttpHandle,
        _jwt: &mut JwtRetriever,
        _request: C8yUpdateSoftwareListResponse,
    ) -> Result<Unit, C8YRestError> {
        todo!()
    }

    async fn upload_log_binary(
        &mut self,
        _http: &mut HttpHandle,
        _jwt: &mut JwtRetriever,
        _request: UploadLogBinary,
    ) -> Result<EventId, C8YRestError> {
        todo!()
    }

    async fn upload_config_file(
        &mut self,
        _http: &mut HttpHandle,
        _jwt: &mut JwtRetriever,
        _request: UploadConfigFile,
    ) -> Result<EventId, C8YRestError> {
        todo!()
    }

    async fn download_file(
        &mut self,
        _http: &mut HttpHandle,
        _jwt: &mut JwtRetriever,
        _request: DownloadFile,
    ) -> Result<Unit, C8YRestError> {
        todo!()
    }
}
