use crate::credentials::JwtRequest;
use crate::credentials::JwtResult;
use crate::credentials::JwtRetriever;
use crate::messages::C8YConnectionError;
use crate::messages::C8YRestError;
use crate::messages::C8YRestRequest;
use crate::messages::C8YRestResult;
use crate::messages::DownloadFile;
use crate::messages::EventId;
use crate::messages::Unit;
use crate::messages::UploadConfigFile;
use crate::messages::UploadLogBinary;
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
use download::Auth;
use download::DownloadInfo;
use download::Downloader;
use log::debug;
use log::error;
use log::info;
use std::collections::HashMap;
use std::time::Duration;
use tedge_actors::fan_in_message_type;
use tedge_actors::Actor;
use tedge_actors::ClientMessageBox;
use tedge_actors::MessageReceiver;
use tedge_actors::RuntimeError;
use tedge_actors::RuntimeRequest;
use tedge_actors::Sender;
use tedge_actors::ServerMessageBox;
use tedge_http_ext::HttpRequest;
use tedge_http_ext::HttpRequestBuilder;
use tedge_http_ext::HttpResponseExt;
use tedge_http_ext::HttpResult;

const RETRY_TIMEOUT_SECS: u64 = 20;

pub struct C8YHttpProxyActor {
    end_point: C8yEndPoint,
    child_devices: HashMap<String, String>,
    peers: C8YHttpProxyMessageBox,
}

pub struct C8YHttpProxyMessageBox {
    /// Connection to the clients
    pub(crate) clients: ServerMessageBox<C8YRestRequest, C8YRestResult>,

    /// Connection to an HTTP actor
    pub(crate) http: ClientMessageBox<HttpRequest, HttpResult>,

    /// Connection to a JWT token retriever
    pub(crate) jwt: JwtRetriever,
}

#[derive(Debug)]
pub struct C8YRestRequestWithClientId(usize, C8YRestRequest);

#[derive(Debug)]
pub struct C8YRestResponseWithClientId(usize, C8YRestResult);

fan_in_message_type!(C8YHttpProxyInput[C8YRestRequestWithClientId, HttpResult, JwtResult] : Debug);
fan_in_message_type!(C8YHttpProxyOutput[C8YRestResponseWithClientId, HttpRequest, JwtRequest] : Debug);

#[async_trait]
impl Actor for C8YHttpProxyActor {
    fn name(&self) -> &str {
        "C8Y-REST"
    }

    async fn run(&mut self) -> Result<(), RuntimeError> {
        self.init().await.map_err(Box::new)?;

        while let Some((client_id, request)) = self.peers.clients.recv().await {
            let result = match request {
                C8YRestRequest::GetJwtToken(_) => {
                    self.get_jwt_token().await.map(|response| response.into())
                }

                C8YRestRequest::C8yCreateEvent(request) => self
                    .create_event(request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::C8yUpdateSoftwareListResponse(request) => self
                    .send_software_list_http(request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::UploadLogBinary(request) => self
                    .upload_log_binary(request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::UploadConfigFile(request) => self
                    .upload_config_file(request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::DownloadFile(request) => self
                    .download_file(request)
                    .await
                    .map(|response| response.into()),
            };
            self.peers.clients.send((client_id, result)).await?;
        }
        Ok(())
    }
}

impl C8YHttpProxyActor {
    pub fn new(config: C8YHttpConfig, message_box: C8YHttpProxyMessageBox) -> Self {
        let unknown_internal_id = "";
        let end_point = C8yEndPoint::new(&config.c8y_host, &config.device_id, unknown_internal_id);
        let child_devices = HashMap::default();
        C8YHttpProxyActor {
            end_point,
            child_devices,
            peers: message_box,
        }
    }

    async fn init(&mut self) -> Result<(), C8YConnectionError> {
        info!(target: self.name(), "start initialisation");
        while self.end_point.get_c8y_internal_id().is_empty() {
            if let Err(error) = self.try_get_and_set_internal_id().await {
                error!(
                    "An error occurred while retrieving internal Id, operation will retry in {} seconds\n Error: {:?}",
                    RETRY_TIMEOUT_SECS, error
                );

                match tokio::time::timeout(
                    Duration::from_secs(RETRY_TIMEOUT_SECS),
                    self.peers.clients.recv_signal(),
                )
                .await
                {
                    Ok(Some(RuntimeRequest::Shutdown)) => {
                        // Give up as requested
                        return Err(C8YConnectionError::Interrupted);
                    }
                    Err(_timeout) => {
                        // No interruption raised, so just continue
                        continue;
                    }
                    Ok(None) => {
                        // This actor is not connected to the runtime and will never be interrupted
                        tokio::time::sleep(Duration::from_secs(RETRY_TIMEOUT_SECS)).await;
                        continue;
                    }
                }
            };
        }
        info!(target: self.name(), "initialisation done.");
        Ok(())
    }

    async fn try_get_and_set_internal_id(&mut self) -> Result<(), C8YRestError> {
        let internal_id = self.try_get_internal_id(None).await?;
        self.end_point.set_c8y_internal_id(internal_id);
        Ok(())
    }

    async fn get_c8y_internal_child_id(
        &mut self,
        child_device_id: String,
    ) -> Result<String, C8YRestError> {
        if let Some(c8y_internal_id) = self.child_devices.get(&child_device_id) {
            Ok(c8y_internal_id.clone())
        } else {
            let c8y_internal_id = self.try_get_internal_id(Some(&child_device_id)).await?;
            self.child_devices
                .insert(child_device_id, c8y_internal_id.clone());
            Ok(c8y_internal_id)
        }
    }

    async fn try_get_internal_id(
        &mut self,
        device_id: Option<&str>,
    ) -> Result<String, C8YRestError> {
        let url_get_id = self.end_point.get_url_for_get_id(device_id);

        let request_internal_id = HttpRequestBuilder::get(url_get_id);
        let res = self.execute(request_internal_id).await?;
        let res = res.error_for_status()?;

        let internal_id_response: InternalIdResponse = res.json().await?;
        let internal_id = internal_id_response.id();
        Ok(internal_id)
    }

    async fn execute(
        &mut self,
        request_builder: HttpRequestBuilder,
    ) -> Result<HttpResult, C8YRestError> {
        // Get a JWT token to authenticate the device
        let request_builder = if let Ok(token) = self.peers.jwt.await_response(()).await? {
            request_builder.bearer_auth(token)
        } else {
            return Err(C8YRestError::CustomError("JWT token not available".into()));
        };

        // TODO Add timeout
        // TODO Manage 403 errors
        let request = request_builder.build()?;
        Ok(self.peers.http.await_response(request).await?)
    }

    async fn create_event(
        &mut self,
        mut c8y_event: C8yCreateEvent,
    ) -> Result<EventId, C8YRestError> {
        if c8y_event.source.is_none() {
            c8y_event.source = Some(C8yManagedObject {
                id: self.end_point.get_c8y_internal_id().to_string(),
            });
        }
        self.send_event_internal(c8y_event).await
    }

    async fn send_software_list_http(
        &mut self,
        software_list: C8yUpdateSoftwareListResponse,
    ) -> Result<Unit, C8YRestError> {
        let url = self.end_point.get_url_for_sw_list();
        let req_builder = HttpRequestBuilder::put(url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&software_list);
        let http_result = self.execute(req_builder).await?;
        let _ = http_result.error_for_status()?;
        Ok(())
    }

    async fn upload_log_binary(
        &mut self,
        request: UploadLogBinary,
    ) -> Result<EventId, C8YRestError> {
        let log_file_event = self
            .create_event_request(request.log_type, None, None, request.child_device_id)
            .await?;

        let event_response_id = self.send_event_internal(log_file_event).await?;

        let binary_upload_event_url = self
            .end_point
            .get_url_for_event_binary_upload(&event_response_id);

        let req_builder = HttpRequestBuilder::post(binary_upload_event_url.clone())
            .header("Accept", "application/json")
            .header("Content-Type", "text/plain")
            .body(request.log_content);
        let http_result = self.execute(req_builder).await?.unwrap();

        if !http_result.status().is_success() {
            Err(C8YRestError::CustomError("Upload failed".into()))
        } else {
            Ok(binary_upload_event_url)
        }
    }

    async fn upload_config_file(
        &mut self,
        request: UploadConfigFile,
    ) -> Result<EventId, C8YRestError> {
        // read the config file contents
        let config_content = std::fs::read_to_string(request.config_path)
            .map_err(<std::io::Error as Into<SMCumulocityMapperError>>::into)?;

        let config_file_event = self
            .create_event_request(request.config_type, None, None, request.child_device_id)
            .await?;

        debug!(target: self.name(), "Creating config event: {:?}", config_file_event);
        let event_response_id = self.send_event_internal(config_file_event).await?;
        debug!(target: self.name(), "Config event created with id: {:?}", event_response_id);

        let binary_upload_event_url = self
            .end_point
            .get_url_for_event_binary_upload(&event_response_id);
        let req_builder = HttpRequestBuilder::post(binary_upload_event_url.clone())
            .header("Accept", "application/json")
            .header("Content-Type", "text/plain")
            .body(config_content.to_string());
        debug!(target: self.name(), "Uploading config file to URL: {}", binary_upload_event_url);
        let http_result = self.execute(req_builder).await?.unwrap();

        if !http_result.status().is_success() {
            Err(C8YRestError::CustomError("Upload failed".into()))
        } else {
            Ok(binary_upload_event_url)
        }
    }

    async fn get_jwt_token(&mut self) -> Result<String, C8YRestError> {
        if let Ok(token) = self.peers.jwt.await_response(()).await? {
            Ok(token)
        } else {
            Err(C8YRestError::CustomError("JWT token not available".into()))
        }
    }

    async fn download_file(&mut self, request: DownloadFile) -> Result<Unit, C8YRestError> {
        let mut download_info: DownloadInfo = request.download_url.as_str().into();
        // If the provided url is c8y, add auth
        if self
            .end_point
            .url_is_in_my_tenant_domain(download_info.url())
        {
            let token = self.get_jwt_token().await?;
            download_info.auth = Some(Auth::new_bearer(token.as_str()));
        }

        debug!(target: self.name(), "Downloading from: {:?}", download_info.url());
        let downloader: Downloader =
            Downloader::with_permission(request.file_path, request.file_permissions);
        downloader.download(&download_info).await?;

        Ok(())
    }

    async fn create_event_request(
        &mut self,
        event_type: String,
        event_text: Option<String>,
        event_time: Option<OffsetDateTime>,
        child_device_id: Option<String>,
    ) -> Result<C8yCreateEvent, C8YRestError> {
        let device_internal_id = if let Some(device_id) = child_device_id {
            self.get_c8y_internal_child_id(device_id).await?
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
        c8y_event: C8yCreateEvent,
    ) -> Result<EventId, C8YRestError> {
        let create_event_url = self.end_point.get_url_for_create_event();

        let req_builder = HttpRequestBuilder::post(create_event_url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&c8y_event);
        let http_result = self.execute(req_builder).await?;
        let http_response = http_result.error_for_status()?;
        let event_response: C8yEventResponse = http_response.json().await?;
        Ok(event_response.id)
    }
}
