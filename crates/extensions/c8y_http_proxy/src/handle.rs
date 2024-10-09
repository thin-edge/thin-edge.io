use crate::messages::C8YRestError;
use crate::messages::C8YRestRequest;
use crate::messages::C8YRestResponse;
use crate::messages::C8YRestResult;
use crate::messages::CreateEvent;
use crate::messages::GetFreshJwtToken;
use crate::messages::GetJwtToken;
use crate::messages::SoftwareListResponse;
use crate::messages::UploadFile;
use crate::messages::UploadLogBinary;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::ClientMessageBox;
use tedge_actors::Service;

use super::messages::DownloadFile;

/// Handle to the C8YHttpProxy
#[derive(Clone)]
pub struct C8YHttpProxy {
    c8y: ClientMessageBox<C8YRestRequest, C8YRestResult>,
}

impl C8YHttpProxy {
    pub fn new(proxy_builder: &mut impl Service<C8YRestRequest, C8YRestResult>) -> Self {
        let c8y = ClientMessageBox::new(proxy_builder);
        C8YHttpProxy { c8y }
    }

    pub async fn get_jwt_token(&mut self) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = GetJwtToken.into();

        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::EventId(id)) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn get_fresh_jwt_token(&mut self) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = GetFreshJwtToken.into();

        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::EventId(id)) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn send_event(&mut self, c8y_event: CreateEvent) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = c8y_event.into();
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::EventId(id)) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn send_software_list_http(
        &mut self,
        c8y_software_list: C8yUpdateSoftwareListResponse,
        device_id: String,
    ) -> Result<(), C8YRestError> {
        let request: C8YRestRequest = SoftwareListResponse {
            c8y_software_list,
            device_id,
        }
        .into();

        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::Unit(_)) => Ok(()),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn upload_log_binary(
        &mut self,
        log_type: &str,
        log_content: &str,
        device_id: String,
    ) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = UploadLogBinary {
            log_type: log_type.to_string(),
            log_content: log_content.to_string(),
            device_id,
        }
        .into();
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::EventId(id)) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn upload_file(
        &mut self,
        file_path: &Path,
        file_type: &str,
        device_id: String,
    ) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = UploadFile {
            file_path: file_path.to_owned(),
            file_type: file_type.to_string(),
            device_id,
        }
        .into();
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::Url(url)) => Ok(url.0),
            unexpected => Err(unexpected.into()),
        }
    }

    pub async fn download_file(
        &mut self,
        download_url: &str,
        file_path: PathBuf,
    ) -> Result<(), C8YRestError> {
        let request: C8YRestRequest = DownloadFile {
            download_url: download_url.into(),
            file_path,
        }
        .into();
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::Unit(())) => Ok(()),
            unexpected => Err(unexpected.into()),
        }
    }
}
