use crate::c8y_http_proxy::messages::C8YRestError;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use crate::c8y_http_proxy::messages::C8YRestResult;
use crate::c8y_http_proxy::messages::UploadConfigFile;
use crate::c8y_http_proxy::messages::UploadLogBinary;
use crate::c8y_http_proxy::C8YConnectionBuilder;
use std::path::Path;
use std::path::PathBuf;
use tedge_actors::NoConfig;
use tedge_actors::RequestResponseHandler;
use tedge_utils::file::PermissionEntry;

use super::messages::DownloadFile;

/// Handle to the C8YHttpProxy
pub struct C8YHttpProxy {
    c8y: RequestResponseHandler<C8YRestRequest, C8YRestResult>,
}

impl C8YHttpProxy {
    pub fn new(client_name: &str, proxy_builder: &mut impl C8YConnectionBuilder) -> Self {
        let c8y = RequestResponseHandler::new(client_name, proxy_builder, NoConfig);
        C8YHttpProxy { c8y }
    }

    /* Will be used by the mapper
    pub async fn send_event(&mut self, c8y_event: C8yCreateEvent) -> Result<String, C8YRestError> {
        let request: C8YRestRequest = c8y_event.into();
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::EventId(id)) => Ok(id),
            unexpected => Err(unexpected.into()),
        }
    } */

    /* Will be used by the mapper
    pub async fn send_software_list_http(
        &mut self,
        c8y_software_list: C8yUpdateSoftwareListResponse,
    ) -> Result<(), C8YRestError> {
        let request: C8YRestRequest = c8y_software_list.into();
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::Unit(_)) => Ok(()),
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
        match self.c8y.await_response(request).await? {
            Ok(C8YRestResponse::EventId(id)) => Ok(id),
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
