use crate::credentials::JwtRequest;
use crate::credentials::JwtResult;
use crate::credentials::JwtRetriever;
use crate::messages::C8YConnectionError;
use crate::messages::C8YRestError;
use crate::messages::C8YRestRequest;
use crate::messages::C8YRestResult;
use crate::messages::CreateEvent;
use crate::messages::DownloadFile;
use crate::messages::EventId;
use crate::messages::SoftwareListResponse;
use crate::messages::Unit;
use crate::messages::UploadFile;
use crate::messages::UploadLogBinary;
use crate::messages::Url;
use crate::C8YHttpConfig;
use anyhow::Context;
use async_trait::async_trait;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::C8yManagedObject;
use c8y_api::json_c8y::InternalIdResponse;
use c8y_api::OffsetDateTime;
use download::Auth;
use download::DownloadInfo;
use download::Downloader;
use http::status::StatusCode;
use log::debug;
use log::error;
use log::info;
use std::collections::HashMap;
use std::future::ready;
use std::future::Future;
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
use tokio::time::sleep;

const RETRY_TIMEOUT_SECS: u64 = 20;

pub struct C8YHttpProxyActor {
    config: C8YHttpConfig,
    pub(crate) end_point: C8yEndPoint,
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

    async fn run(mut self) -> Result<(), RuntimeError> {
        self.init().await.map_err(Box::new)?;

        while let Some((client_id, request)) = self.peers.clients.recv().await {
            let result = match request {
                C8YRestRequest::GetJwtToken(_) => self
                    .get_and_set_jwt_token()
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::GetFreshJwtToken(_) => {
                    self.end_point.token = None;
                    self.get_and_set_jwt_token()
                        .await
                        .map(|response| response.into())
                }

                C8YRestRequest::CreateEvent(request) => self
                    .create_event(request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::SoftwareListResponse(request) => self
                    .send_software_list_http(request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::UploadLogBinary(request) => self
                    .upload_log_binary(request)
                    .await
                    .map(|response| response.into()),

                C8YRestRequest::UploadFile(request) => {
                    self.upload_file(request).await.map(|url| url.into())
                }

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
        let end_point = C8yEndPoint::new(&config.c8y_host, &config.device_id);
        C8YHttpProxyActor {
            config,
            end_point,
            peers: message_box,
        }
    }

    async fn init(&mut self) -> Result<(), C8YConnectionError> {
        info!(target: self.name(), "start initialisation");

        while self
            .end_point
            .get_internal_id(self.end_point.device_id.clone())
            .is_err()
        {
            if let Err(error) = self
                .get_and_set_internal_id(self.end_point.device_id.clone())
                .await
            {
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

    async fn get_and_set_internal_id(&mut self, device_id: String) -> Result<(), C8YRestError> {
        let internal_id = self.try_get_internal_id(device_id.clone()).await?;
        self.end_point.set_internal_id(device_id, internal_id);

        Ok(())
    }

    pub(crate) async fn try_get_internal_id(
        &mut self,
        device_id: String,
    ) -> Result<String, C8YRestError> {
        let url_get_id: String = self.end_point.get_url_for_internal_id(&device_id);
        if self.end_point.token.is_none() {
            self.get_fresh_token().await?;
        }

        let mut attempt = 0;
        let mut token_refreshed = false;
        loop {
            attempt += 1;
            let request = HttpRequestBuilder::get(&url_get_id)
                .bearer_auth(self.end_point.token.clone().unwrap_or_default())
                .build()?;

            match self.peers.http.await_response(request).await? {
                Ok(response) => {
                    match response.status() {
                        StatusCode::OK => {
                            let internal_id_response: InternalIdResponse = response.json().await?;
                            let internal_id = internal_id_response.id();

                            return Ok(internal_id);
                        }
                        StatusCode::NOT_FOUND => {
                            if attempt > 3 {
                                error!("Failed to fetch internal id for {device_id} even after multiple attempts");
                                response.error_for_status()?;
                            }
                            info!("Re-fetching internal id for {device_id}, attempt: {attempt}");
                            sleep(self.config.retry_interval).await;
                            continue;
                        }
                        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                            if token_refreshed {
                                error!("Failed to fetch internal id for {device_id} even with fresh token");
                                response.error_for_status()?;
                            }
                            info!("Re-fetching internal id for {device_id} with fresh token");
                            self.get_fresh_token().await?;
                            token_refreshed = true;
                            continue;
                        }
                        code => {
                            return Err(C8YRestError::FromHttpError(
                                tedge_http_ext::HttpError::HttpStatusError(code),
                            ))
                        }
                    }
                }

                Err(e) => return Err(C8YRestError::FromHttpError(e)),
            }
        }
    }

    async fn execute<Fut: Future<Output = Result<HttpRequestBuilder, C8YRestError>>>(
        &mut self,
        device_id: String,
        build_request: impl Fn(&C8yEndPoint) -> Fut,
    ) -> Result<HttpResult, C8YRestError> {
        let request_builder = build_request(&self.end_point);
        let request = request_builder
            .await?
            .bearer_auth(self.end_point.token.clone().unwrap_or_default())
            .build()?;

        let resp = self.peers.http.await_response(request).await?;
        match resp {
            Ok(response) => match response.status() {
                StatusCode::OK | StatusCode::CREATED => Ok(Ok(response)),
                StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                    self.try_request_with_fresh_token(build_request).await
                }
                StatusCode::NOT_FOUND => {
                    self.try_request_with_fresh_internal_id(device_id, build_request)
                        .await
                }
                code => Err(C8YRestError::FromHttpError(
                    tedge_http_ext::HttpError::HttpStatusError(code),
                )),
            },

            Err(e) => Err(C8YRestError::FromHttpError(e)),
        }
    }

    async fn get_fresh_token(&mut self) -> Result<String, C8YRestError> {
        self.end_point.token = None;
        self.get_and_set_jwt_token().await
    }

    async fn try_request_with_fresh_token<
        Fut: Future<Output = Result<HttpRequestBuilder, C8YRestError>>,
    >(
        &mut self,
        build_request: impl Fn(&C8yEndPoint) -> Fut,
    ) -> Result<HttpResult, C8YRestError> {
        // get new token not the cached one
        self.get_fresh_token().await?;
        // build the request
        let request_builder = build_request(&self.end_point);
        let request = request_builder
            .await?
            .bearer_auth(self.end_point.token.clone().unwrap_or_default())
            .build()?;
        // retry the request
        Ok(self.peers.http.await_response(request).await?)
    }

    async fn try_request_with_fresh_internal_id<
        Fut: Future<Output = Result<HttpRequestBuilder, C8YRestError>>,
    >(
        &mut self,
        device_id: String,
        build_request: impl Fn(&C8yEndPoint) -> Fut,
    ) -> Result<HttpResult, C8YRestError> {
        // get new internal id not the cached one
        self.get_and_set_internal_id(device_id).await?;

        let request_builder = build_request(&self.end_point);
        let request = request_builder
            .await?
            .bearer_auth(self.end_point.token.clone().unwrap_or_default())
            .build()?;
        Ok(self.peers.http.await_response(request).await?)
    }

    pub(crate) async fn create_event(
        &mut self,
        c8y_event: CreateEvent,
    ) -> Result<EventId, C8YRestError> {
        let create_event = |internal_id: String| -> C8yCreateEvent {
            C8yCreateEvent {
                source: Some(C8yManagedObject { id: internal_id }),
                event_type: c8y_event.event_type.clone(),
                time: c8y_event.time,
                text: c8y_event.text.clone(),
                extras: c8y_event.extras.clone(),
            }
        };

        self.send_event_internal(c8y_event.device_id, create_event)
            .await
    }

    async fn send_software_list_http(
        &mut self,
        software_list: SoftwareListResponse,
    ) -> Result<Unit, C8YRestError> {
        let device_id = software_list.device_id;

        // Get and set child device internal id
        if device_id.ne(&self.end_point.device_id)
            && self.end_point.get_internal_id(device_id.clone()).is_err()
        {
            self.get_and_set_internal_id(device_id.clone()).await?;
        }

        let build_request = |end_point: &C8yEndPoint| {
            let internal_id = end_point
                .get_internal_id(device_id.clone())
                .map_err(|e| C8YRestError::CustomError(e.to_string()));
            let url = internal_id.map(|id| end_point.get_url_for_sw_list(id));
            async {
                Ok::<_, C8YRestError>(
                    HttpRequestBuilder::put(url?)
                        .header("Accept", "application/json")
                        .header("Content-Type", "application/json")
                        .json(&software_list.c8y_software_list),
                )
            }
        };

        let http_result = self.execute(device_id.clone(), build_request).await?;
        http_result.error_for_status()?;
        Ok(())
    }

    async fn upload_log_binary(
        &mut self,
        request: UploadLogBinary,
    ) -> Result<EventId, C8YRestError> {
        let device_id = request.device_id;
        let create_event = |internal_id: String| -> C8yCreateEvent {
            C8yCreateEvent {
                source: Some(C8yManagedObject { id: internal_id }),
                event_type: request.log_type.clone(),
                time: OffsetDateTime::now_utc(),
                text: request.log_type.clone(),
                extras: HashMap::new(),
            }
        };
        let event_response_id = self
            .send_event_internal(device_id.clone(), create_event)
            .await?;

        let build_request = |end_point: &C8yEndPoint| {
            let binary_upload_event_url =
                end_point.get_url_for_event_binary_upload(&event_response_id);
            ready(Ok::<_, C8YRestError>(
                HttpRequestBuilder::post(&binary_upload_event_url)
                    .header("Accept", "application/json")
                    .header("Content-Type", "text/plain")
                    .body(request.log_content.clone()),
            ))
        };

        let http_result = self.execute(device_id.clone(), build_request).await??;

        if !http_result.status().is_success() {
            Err(C8YRestError::CustomError("Upload failed".into()))
        } else {
            Ok(self
                .end_point
                .get_url_for_event_binary_upload(&event_response_id))
        }
    }

    async fn upload_file(&mut self, request: UploadFile) -> Result<Url, C8YRestError> {
        let device_id = request.device_id;

        let create_event = |internal_id: String| -> C8yCreateEvent {
            C8yCreateEvent {
                source: Some(C8yManagedObject { id: internal_id }),
                event_type: request.file_type.clone(),
                time: OffsetDateTime::now_utc(),
                text: request.file_type.clone(),
                extras: HashMap::new(),
            }
        };

        let event_response_id = self
            .send_event_internal(device_id.clone(), create_event)
            .await?;
        debug!(target: self.name(), "File event created with id: {:?}", event_response_id);

        let build_request = |end_point: &C8yEndPoint| {
            let binary_upload_event_url =
                end_point.get_url_for_event_binary_upload(&event_response_id);

            async {
                // TODO: Upload the file as a multi-part stream
                let file_content =
                    tokio::fs::read(&request.file_path).await.with_context(|| {
                        format!(
                            "Reading file {} for upload failed",
                            request.file_path.display()
                        )
                    })?;

                let (content_type, body) = match String::from_utf8(file_content) {
                    Ok(text) => ("text/plain", hyper::Body::from(text)),
                    Err(not_text) => ("application/octet-stream", not_text.into_bytes().into()),
                };

                Ok::<_, C8YRestError>(
                    HttpRequestBuilder::post(binary_upload_event_url)
                        .header("Accept", "application/json")
                        .header("Content-Type", content_type)
                        .body(body),
                )
            }
        };
        info!(target: self.name(), "Uploading file to URL: {}", self.end_point
        .get_url_for_event_binary_upload(&event_response_id));
        let http_result = self.execute(device_id.clone(), build_request).await??;

        if !http_result.status().is_success() {
            Err(C8YRestError::CustomError("Upload failed".into()))
        } else {
            Ok(Url(self
                .end_point
                .get_url_for_event_binary_upload(&event_response_id)))
        }
    }

    async fn get_and_set_jwt_token(&mut self) -> Result<String, C8YRestError> {
        match self.end_point.token.clone() {
            Some(token) => Ok(token),
            None => {
                if let Ok(token) = self.peers.jwt.await_response(()).await? {
                    self.end_point.token = Some(token.clone());
                    Ok(token)
                } else {
                    Err(C8YRestError::CustomError("JWT token not available".into()))
                }
            }
        }
    }

    async fn download_file(&mut self, request: DownloadFile) -> Result<Unit, C8YRestError> {
        let mut download_info: DownloadInfo = request.download_url.as_str().into();
        // If the provided url is c8y, add auth
        if self
            .end_point
            .maybe_tenant_url(download_info.url())
            .is_some()
        {
            let token = self.get_and_set_jwt_token().await?;
            download_info.auth = Some(Auth::new_bearer(token.as_str()));
        }

        info!(target: self.name(), "Downloading from: {:?}", download_info.url());
        let downloader: Downloader = Downloader::with_permission(
            request.file_path,
            request.file_permissions,
            self.config.identity.clone(),
        );
        downloader.download(&download_info).await?;

        Ok(())
    }

    async fn send_event_internal(
        &mut self,
        device_id: String,
        create_event: impl Fn(String) -> C8yCreateEvent,
    ) -> Result<EventId, C8YRestError> {
        // Get and set child device internal id
        if device_id.ne(&self.end_point.device_id) {
            self.get_and_set_internal_id(device_id.clone()).await?;
        }

        let build_request = |end_point: &C8yEndPoint| {
            let create_event_url = end_point.get_url_for_create_event();
            let internal_id = end_point
                .get_internal_id(device_id.clone())
                .map_err(|e| C8YRestError::CustomError(e.to_string()));

            async {
                let updated_c8y_event = create_event(internal_id?);

                Ok::<_, C8YRestError>(
                    HttpRequestBuilder::post(create_event_url)
                        .header("Accept", "application/json")
                        .header("Content-Type", "application/json")
                        .json(&updated_c8y_event),
                )
            }
        };

        let http_result = self.execute(device_id.clone(), build_request).await?;
        let http_response = http_result.error_for_status()?;
        let event_response: C8yEventResponse = http_response.json().await?;
        Ok(event_response.id)
    }
}
