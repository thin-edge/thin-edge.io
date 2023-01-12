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
use crate::C8YHttpConfig;
use async_trait::async_trait;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::C8yManagedObject;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use c8y_api::json_c8y::InternalIdResponse;
use c8y_api::smartrest::error::SMCumulocityMapperError;
use c8y_api::OffsetDateTime;
use hyper::body;
use log::error;
use log::info;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::time::Duration;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ChannelError;
use tedge_actors::MessageBox;
use tedge_actors::ServiceMessageBox;
use tedge_http_ext::HttpHandle;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResult;

const RETRY_TIMEOUT_SECS: u64 = 60;

pub(crate) struct C8YHttpProxyActor {
    end_point: C8yEndPoint,
    child_devices: HashMap<String, String>,
}

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

    fn turn_logging_on(&mut self, _on: bool) {
        todo!()
    }

    fn name(&self) -> &str {
        "C8Y-REST"
    }

    fn logging_is_on(&self) -> bool {
        true
    }
}

impl C8YHttpProxyActor {
    pub fn new(config: C8YHttpConfig) -> Self {
        let unknown_internal_id = "";
        let end_point = C8yEndPoint::new(&config.c8y_host, &config.device_id, unknown_internal_id);
        let child_devices = HashMap::default();
        C8YHttpProxyActor {
            end_point,
            child_devices,
        }
    }

    pub async fn run(mut self, mut messages: C8YHttpProxyMessageBox) -> Result<(), ChannelError> {
        let clients = &mut messages.clients;
        let http = &mut messages.http;
        let jwt = &mut messages.jwt;

        self.init(http, jwt).await;

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
            clients.send((client_id, result)).await?;
        }
        Ok(())
    }

    async fn init(&mut self, http: &mut HttpHandle, jwt: &mut JwtRetriever) {
        info!(target: self.name(), "start initialisation");
        while self.end_point.get_c8y_internal_id().is_empty() {
            if let Err(error) = self.try_get_and_set_internal_id(http, jwt).await {
                error!(
                    "An error occurred while retrieving internal Id, operation will retry in {} seconds\n Error: {:?}",
                    RETRY_TIMEOUT_SECS, error
                );

                tokio::time::sleep(Duration::from_secs(RETRY_TIMEOUT_SECS)).await;
                continue;
            };
        }
        info!(target: self.name(), "initialisation done.");
    }

    async fn try_get_and_set_internal_id(
        &mut self,
        http: &mut HttpHandle,
        jwt: &mut JwtRetriever,
    ) -> Result<(), C8YRestError> {
        let internal_id = self.try_get_internal_id(http, jwt, None).await?;
        self.end_point.set_c8y_internal_id(internal_id);
        Ok(())
    }

    async fn get_c8y_internal_child_id(
        &mut self,
        http: &mut HttpHandle,
        jwt: &mut JwtRetriever,
        child_device_id: String,
    ) -> Result<String, C8YRestError> {
        if let Some(c8y_internal_id) = self.child_devices.get(&child_device_id) {
            Ok(c8y_internal_id.clone())
        } else {
            let c8y_internal_id = self
                .try_get_internal_id(http, jwt, Some(&child_device_id))
                .await?;
            self.child_devices
                .insert(child_device_id, c8y_internal_id.clone());
            Ok(c8y_internal_id)
        }
    }

    async fn try_get_internal_id(
        &mut self,
        http: &mut HttpHandle,
        jwt: &mut JwtRetriever,
        device_id: Option<&str>,
    ) -> Result<String, C8YRestError> {
        let url_get_id = self.end_point.get_url_for_get_id(device_id);

        let request_internal_id = HttpRequestBuilder::get(url_get_id);
        let res = self.execute(http, jwt, request_internal_id).await?.unwrap();

        let body_bytes = body::to_bytes(res.into_body()).await.unwrap();
        let body_string =
            String::from_utf8(body_bytes.to_vec()).expect("response was not valid utf-8");

        let internal_id_response: InternalIdResponse =
            serde_json::from_str(body_string.as_str()).expect("FIXME: JSON parsing failed");
        let internal_id = internal_id_response.id();
        Ok(internal_id)
    }

    async fn execute(
        &mut self,
        http: &mut HttpHandle,
        jwt: &mut JwtRetriever,
        request_builder: HttpRequestBuilder,
    ) -> Result<HttpResult, C8YRestError> {
        // Get a JWT token to authenticate the device
        let request_builder = if let Ok(token) = jwt.await_response(()).await? {
            request_builder.bearer_auth(token)
        } else {
            return Err(C8YRestError::CustomError("JWT token not available".into()));
        };

        // TODO Add timeout
        // TODO Manage 403 errors
        let request = request_builder.build()?;
        Ok(http.await_response(request).await?)
    }

    //FIXME: Move this into HttpResponse as a trait impl
    pub async fn response_as_string(res: HttpResult) -> Result<String, C8YRestError> {
        let body_bytes = body::to_bytes(res.unwrap().into_body()).await?;
        Ok(String::from_utf8(body_bytes.to_vec()).expect("response was not valid utf-8"))
    }

    //FIXME: Move this into HttpResponse as a trait impl
    pub async fn response_as<T: DeserializeOwned>(res: HttpResult) -> Result<T, C8YRestError> {
        let body_str = Self::response_as_string(res).await?;
        Ok(serde_json::from_str(body_str.as_str())?)
    }

    async fn create_event(
        &mut self,
        http: &mut HttpHandle,
        jwt: &mut JwtRetriever,
        mut c8y_event: C8yCreateEvent,
    ) -> Result<EventId, C8YRestError> {
        if c8y_event.source.is_none() {
            c8y_event.source = Some(C8yManagedObject {
                id: self.end_point.get_c8y_internal_id().to_string(),
            });
        }
        self.send_event_internal(http, jwt, c8y_event).await
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
        http: &mut HttpHandle,
        jwt: &mut JwtRetriever,
        request: UploadConfigFile,
    ) -> Result<EventId, C8YRestError> {
        // read the config file contents
        let config_content = std::fs::read_to_string(request.config_path)
            .map_err(<std::io::Error as Into<SMCumulocityMapperError>>::into)?;

        let config_file_event = self
            .create_event_request(
                http,
                jwt,
                request.config_type,
                None,
                None,
                request.child_device_id,
            )
            .await?;

        let event_response_id = self
            .send_event_internal(http, jwt, config_file_event)
            .await?;

        let binary_upload_event_url = self
            .end_point
            .get_url_for_event_binary_upload(&event_response_id);
        let req_builder = HttpRequestBuilder::post(binary_upload_event_url.clone())
            .header("Accept", "application/json")
            .header("Content-Type", "text/plain")
            .body(config_content.to_string());
        let http_result = self.execute(http, jwt, req_builder).await?.unwrap();

        if !http_result.status().is_success() {
            Err(C8YRestError::CustomError("Upload failed".into()))
        } else {
            Ok(binary_upload_event_url)
        }
    }

    async fn download_file(
        &mut self,
        _http: &mut HttpHandle,
        _jwt: &mut JwtRetriever,
        _request: DownloadFile,
    ) -> Result<Unit, C8YRestError> {
        todo!()
    }

    async fn create_event_request(
        &mut self,
        http: &mut HttpHandle,
        jwt: &mut JwtRetriever,
        event_type: String,
        event_text: Option<String>,
        event_time: Option<OffsetDateTime>,
        child_device_id: Option<String>,
    ) -> Result<C8yCreateEvent, C8YRestError> {
        let device_internal_id = if let Some(device_id) = child_device_id {
            self.get_c8y_internal_child_id(http, jwt, device_id).await?
        } else {
            self.end_point.get_c8y_internal_id().to_string()
        };

        let c8y_managed_object = C8yManagedObject {
            id: device_internal_id,
        };

        Ok(C8yCreateEvent::new(
            Some(c8y_managed_object),
            event_type.clone(),
            event_time.unwrap_or_else(OffsetDateTime::now_utc),
            event_text.unwrap_or(event_type),
            HashMap::new(),
        ))
    }

    async fn send_event_internal(
        &mut self,
        http: &mut HttpHandle,
        jwt: &mut JwtRetriever,
        c8y_event: C8yCreateEvent,
    ) -> Result<EventId, C8YRestError> {
        let create_event_url = self.end_point.get_url_for_create_event();

        let req_builder = HttpRequestBuilder::post(create_event_url)
            .json(&c8y_event)
            .header("Accept", "application/json");
        let http_result = self.execute(http, jwt, req_builder).await?;
        let event_response = Self::response_as::<C8yEventResponse>(http_result).await?;
        Ok(event_response.id)
    }
}
