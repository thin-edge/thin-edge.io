use crate::c8y_http_proxy::messages::C8YRestError;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use crate::c8y_http_proxy::messages::C8YRestResult;
use crate::c8y_http_proxy::messages::UploadConfigFile;
use crate::c8y_http_proxy::messages::UploadLogBinary;
use crate::c8y_http_proxy::C8YConnectionBuilder;
use mqtt_channel::StreamExt;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::mpsc;
use tedge_actors::Builder;
use tedge_actors::DynSender;
use tedge_actors::LinkError;
use tedge_actors::MessageBoxPort;
use tedge_actors::Sender;
use tedge_utils::file::PermissionEntry;

use super::messages::DownloadFile;

/// Handle to the C8YHttpProxy
pub struct C8YHttpProxy {
    request_sender: DynSender<C8YRestRequest>,
    response_receiver: mpsc::Receiver<C8YRestResult>,
}

impl C8YHttpProxy {
    pub fn new(client_name: &str, proxy_builder: &mut impl C8YConnectionBuilder) -> Self {
        C8YHttpHandleBuilder::new(client_name)
            .connected_to(proxy_builder, ())
            .build()
    }

    /* Will be used by the mapper
    pub async fn send_event(&mut self, c8y_event: C8yCreateEvent) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = c8y_event.into();
        self.request_sender.send(request).await?;
        match self.response_receiver.next().await {
            Some(Ok(C8YRestResponse::EventId(id))) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    } */

    /* Will be used by the mapper
    pub async fn send_software_list_http(
        &mut self,
        c8y_software_list: C8yUpdateSoftwareListResponse,
    ) -> Result<(), C8YRestError> {
        let request: C8YRestRequest = c8y_software_list.into();
        self.request_sender.send(request).await?;
        match self.response_receiver.next().await {
            Some(Ok(C8YRestResponse::Unit(_))) => Ok(()),
            unexpected => Err(unexpected.into()),
        }
    } */

    pub async fn upload_log_binary(
        &mut self,
        log_type: &str,
        log_content: &str,
        child_device_id: Option<String>,
    ) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = UploadLogBinary {
            log_type: log_type.to_string(),
            log_content: log_content.to_string(),
            child_device_id,
        }
        .into();
        self.request_sender.send(request).await?;
        match self.response_receiver.next().await {
            Some(Ok(C8YRestResponse::EventId(id))) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn upload_config_file(
        &mut self,
        config_path: &Path,
        config_type: &str,
        child_device_id: Option<String>,
    ) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = UploadConfigFile {
            config_path: config_path.to_owned(),
            config_type: config_type.to_string(),
            child_device_id,
        }
        .into();
        self.request_sender.send(request).await?;
        match self.response_receiver.next().await {
            Some(Ok(C8YRestResponse::EventId(id))) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn download_file(
        &mut self,
        download_url: &str,
        file_path: PathBuf,
        file_permissions: PermissionEntry,
    ) -> Result<(), C8YRestError> {
        let request: C8YRestRequest = DownloadFile {
            download_url: download_url.into(),
            file_path,
            file_permissions,
        }
        .into();
        self.request_sender.send(request).await?;
        match self.response_receiver.next().await {
            Some(Ok(C8YRestResponse::Unit(()))) => Ok(()),
            unexpected => Err(unexpected.into()),
        }
    }
}

pub(crate) struct C8YHttpHandleBuilder {
    name: String,
    response_sender: mpsc::Sender<C8YRestResult>,
    response_receiver: mpsc::Receiver<C8YRestResult>,
    request_sender: Option<DynSender<C8YRestRequest>>,
}

impl C8YHttpHandleBuilder {
    pub(crate) fn new(name: &str) -> Self {
        let (response_sender, response_receiver) = mpsc::channel(1);
        let request_sender = None;
        C8YHttpHandleBuilder {
            name: name.to_string(),
            response_sender,
            response_receiver,
            request_sender,
        }
    }
}

impl MessageBoxPort<C8YRestRequest, C8YRestResult> for C8YHttpHandleBuilder {
    fn set_request_sender(&mut self, request_sender: DynSender<C8YRestRequest>) {
        self.request_sender = Some(request_sender)
    }

    fn get_response_sender(&self) -> DynSender<C8YRestResult> {
        self.response_sender.sender_clone()
    }
}

impl Builder<C8YHttpProxy> for C8YHttpHandleBuilder {
    type Error = LinkError;

    fn try_build(self) -> Result<C8YHttpProxy, Self::Error> {
        if let Some(request_sender) = self.request_sender {
            Ok(C8YHttpProxy {
                request_sender,
                response_receiver: self.response_receiver,
            })
        } else {
            Err(LinkError::MissingPeer { role: self.name })
        }
    }
}
