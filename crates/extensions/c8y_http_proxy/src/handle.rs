use crate::messages::C8YRestError;
use crate::messages::C8YRestRequest;
use crate::messages::C8YRestResponse;
use crate::messages::C8YRestResult;
use crate::messages::GetFreshJwtToken;
use crate::messages::GetJwtToken;
use crate::messages::SoftwareListResponse;
use crate::messages::UploadConfigFile;
use crate::messages::UploadLogBinary;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yUpdateSoftwareListResponse;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::ClientMessageBox;
use tedge_actors::NoConfig;
use tedge_actors::ServiceProvider;
use tedge_utils::file::PermissionEntry;

use super::messages::DownloadFile;

/// Handle to the C8YHttpProxy
pub struct C8YHttpProxy {
    c8y: ClientMessageBox<C8YRestRequest, C8YRestResult>,
}

impl C8YHttpProxy {
    pub fn new(
        client_name: &str,
        proxy_builder: &mut impl ServiceProvider<C8YRestRequest, C8YRestResult, NoConfig>,
    ) -> Self {
        let c8y = ClientMessageBox::new(client_name, proxy_builder);
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

    pub async fn send_event(&mut self, c8y_event: C8yCreateEvent) -> Result<String, C8YRestError> {
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

    pub async fn upload_config_file(
        &mut self,
        config_path: &Path,
        config_type: &str,
        device_id: String,
    ) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = UploadConfigFile {
            config_path: config_path.to_owned(),
            config_type: config_type.to_string(),
            device_id,
        }
        .into();
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::EventId(id)) => Ok(id),
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
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::Unit(())) => Ok(()),
            unexpected => Err(unexpected.into()),
        }
    }
}
