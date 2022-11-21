use crate::child_device::{
    try_cleanup_config_file_from_file_transfer_repositoy, ConfigOperationMessage,
};
use crate::config_manager::{
    ConfigManager, TimerFuture, CONFIG_CHANGE_TOPIC, DEFAULT_OPERATION_DIR_NAME,
    DEFAULT_PLUGIN_CONFIG_FILE_NAME,
};
use crate::{child_device::ConfigOperationRequest, config::FileEntry};
use crate::{
    child_device::ConfigOperationResponse,
    error::{ChildDeviceConfigManagementError, ConfigManagementError},
};
use crate::{error, PluginConfig};
use agent_interface::OperationStatus;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_serializer::{
    CumulocitySupportedOperations, SmartRest, SmartRestSerializer,
    SmartRestSetOperationToExecuting, SmartRestSetOperationToFailed,
    SmartRestSetOperationToSuccessful, TryIntoOperationStatusMessage,
};
use c8y_api::smartrest::topic::C8yTopic;
use download::{Auth, DownloadInfo, Downloader};
use futures::stream::FuturesUnordered;
use mqtt_channel::{Message, SinkExt, Topic, UnboundedSender};

use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::{fs, path::Path};
use tedge_utils::file::{get_filename, get_metadata, PermissionEntry};
use tokio::sync::Mutex;
use tracing::{info, warn};

pub struct ConfigDownloadManager {
    tedge_device_id: String,
    mqtt_publisher: UnboundedSender<Message>,
    http_client: Arc<Mutex<dyn C8YHttpProxy>>,
    local_http_host: String,
    config_dir: PathBuf,
    tmp_dir: PathBuf,
    unfinished_child_op_timers: FuturesUnordered<TimerFuture<(String, String)>>,
}

impl ConfigDownloadManager {
    pub fn new(
        tedge_device_id: String,
        mqtt_publisher: UnboundedSender<Message>,
        http_client: Arc<Mutex<dyn C8YHttpProxy>>,
        local_http_host: String,
        config_dir: PathBuf,
        tmp_dir: PathBuf,
    ) -> Self {
        ConfigDownloadManager {
            tedge_device_id,
            mqtt_publisher,
            http_client,
            local_http_host,
            config_dir,
            tmp_dir,
            unfinished_child_op_timers: FuturesUnordered::new(),
        }
    }

    pub fn child_ops_pending_timeout(
        &mut self,
    ) -> &mut FuturesUnordered<TimerFuture<(String, String)>> {
        &mut self.unfinished_child_op_timers
    }

    pub async fn handle_config_download_request(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
    ) -> Result<(), anyhow::Error> {
        if smartrest_request.device == self.tedge_device_id {
            self.handle_config_download_request_tedge_device(smartrest_request)
                .await
        } else {
            self.handle_config_download_request_child_device(smartrest_request)
                .await
        }
    }

    pub async fn handle_config_download_request_tedge_device(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
    ) -> Result<(), anyhow::Error> {
        let executing_message = DownloadConfigFileStatusMessage::executing()?;
        self.mqtt_publisher.send(executing_message).await?;

        let target_config_type = smartrest_request.config_type.clone();
        let mut target_file_entry = FileEntry::default();

        let config_file_path = self
            .config_dir
            .join(DEFAULT_OPERATION_DIR_NAME)
            .join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);
        let plugin_config = PluginConfig::new(&config_file_path);
        let download_result = {
            match plugin_config.get_file_entry_from_type(&target_config_type) {
                Ok(file_entry) => {
                    target_file_entry = file_entry;
                    self.download_config_file(
                        smartrest_request.url.as_str(),
                        PathBuf::from(&target_file_entry.path),
                        target_file_entry.file_permissions,
                    )
                    .await
                }
                Err(err) => Err(err.into()),
            }
        };

        match download_result {
            Ok(_) => {
                info!("The configuration download for '{target_config_type}' is successful.");

                let successful_message = DownloadConfigFileStatusMessage::successful(None)?;
                self.mqtt_publisher.send(successful_message).await?;

                let notification_message = get_file_change_notification_message(
                    &target_file_entry.path,
                    &target_config_type,
                );
                self.mqtt_publisher.send(notification_message).await?;
                Ok(())
            }
            Err(err) => {
                error!("The configuration download for '{target_config_type}' failed.",);

                let failed_message = DownloadConfigFileStatusMessage::failed(err.to_string())?;
                self.mqtt_publisher.send(failed_message).await?;
                Err(err)
            }
        }
    }

    /// Map the c8y_DownloadConfigFile request into a tedge/commands/req/config_update command for the child device.
    /// The config file update is shared with the child device via the file transfer service.
    /// The configuration update is downloaded from Cumulocity and is uploaded to the file transfer service,
    /// so that it can be shared with a child device.
    /// A unique URL path for this config file, from the file transfer service, is shared with the child device in the command.
    /// The child device can use this URL to download the config file update from the file transfer service.
    pub async fn handle_config_download_request_child_device(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
    ) -> Result<(), anyhow::Error> {
        let child_id = smartrest_request.device;
        let config_type = smartrest_request.config_type;

        let plugin_config = PluginConfig::new(Path::new(&format!(
            "{}/c8y/{child_id}/c8y-configuration-plugin.toml",
            self.config_dir.display()
        )));

        match plugin_config.get_file_entry_from_type(&config_type) {
            Ok(file_entry) => {
                let config_management = ConfigOperationRequest::Update {
                    child_id: child_id.clone(),
                    file_entry,
                };

                if let Err(err) = self
                    .download_config_file(
                        smartrest_request.url.as_str(),
                        config_management
                            .file_transfer_repository_full_path()
                            .into(),
                        PermissionEntry::new(None, None, None), //no need to change ownership of downloaded file
                    )
                    .await
                {
                    // Fail the operation in the cloud if the file download itself fails
                    // by sending EXECUTING and FAILED responses back to back

                    let c8y_child_topic = Topic::new_unchecked(
                        &C8yTopic::ChildSmartRestResponse(child_id).to_string(),
                    );

                    let executing_msg = Message::new(
                        &c8y_child_topic,
                        DownloadConfigFileStatusMessage::status_executing()?,
                    );
                    self.mqtt_publisher.send(executing_msg).await?;

                    let failure_reason = format!(
                        "Downloading the config file update from {} failed with {}",
                        smartrest_request.url, err
                    );
                    let failed_msg = Message::new(
                        &c8y_child_topic,
                        DownloadConfigFileStatusMessage::status_failed(failure_reason)?,
                    );
                    self.mqtt_publisher.send(failed_msg).await?;
                } else {
                    let config_update_req_msg = Message::new(
                        &config_management.operation_request_topic(),
                        config_management.operation_request_payload(&self.local_http_host)?,
                    );
                    self.mqtt_publisher.send(config_update_req_msg).await?;

                    self.unfinished_child_op_timers.push(Box::pin(
                        ConfigManager::timer_for_child_op(child_id, config_type),
                    ));
                }
            }
            Err(ConfigManagementError::InvalidRequestedConfigType { config_type }) => {
                warn!(
                    "Ignoring the config operation request for unknown config type: {config_type}"
                );
            }
            Err(err) => return Err(err)?,
        }

        Ok(())
    }

    pub fn handle_child_device_config_update_response(
        &self,
        config_response: &ConfigOperationResponse,
    ) -> Result<Message, ChildDeviceConfigManagementError> {
        let c8y_child_topic = Topic::new_unchecked(&config_response.get_child_topic());

        let child_device_payload = config_response.get_payload();

        if let Some(operation_status) = child_device_payload.status {
            match operation_status {
                OperationStatus::Successful => {
                    // Cleanup the downloaded file after the operation completes
                    try_cleanup_config_file_from_file_transfer_repositoy(config_response);
                    let successful_status_payload =
                        DownloadConfigFileStatusMessage::status_successful(None)?;
                    Ok(Message::new(&c8y_child_topic, successful_status_payload))
                }
                OperationStatus::Failed => {
                    // Cleanup the downloaded file after the operation completes
                    try_cleanup_config_file_from_file_transfer_repositoy(config_response);
                    if let Some(error_message) = &child_device_payload.reason {
                        let failed_status_payload =
                            DownloadConfigFileStatusMessage::status_failed(error_message.clone())?;
                        Ok(Message::new(&c8y_child_topic, failed_status_payload))
                    } else {
                        let default_error_message =
                            String::from("No fail reason provided by child device.");
                        let failed_status_payload =
                            DownloadConfigFileStatusMessage::status_failed(default_error_message)?;
                        Ok(Message::new(&c8y_child_topic, failed_status_payload))
                    }
                }
                OperationStatus::Executing => {
                    let executing_status_payload =
                        DownloadConfigFileStatusMessage::status_executing()?;
                    Ok(Message::new(&c8y_child_topic, executing_status_payload))
                }
            }
        } else {
            Err(ChildDeviceConfigManagementError::EmptyOperationStatus(
                c8y_child_topic,
            ))
        }
    }

    async fn download_config_file(
        &self,
        download_url: &str,
        file_path: PathBuf,
        file_permissions: PermissionEntry,
    ) -> Result<(), anyhow::Error> {
        // Convert smartrest request to config download request struct
        let mut config_download_request = ConfigDownloadRequest::try_new(
            download_url,
            file_path.clone(),
            self.tmp_dir.clone(),
            file_permissions,
        )?;

        if file_path.exists() {
            // Confirm that the file has write access before any http request attempt
            config_download_request.has_write_access()?;
        } else if let Some(file_parent) = file_path.parent() {
            if !file_parent.exists() {
                fs::create_dir_all(file_parent)?;
            }
        }

        // If the provided url is c8y, add auth
        if self
            .http_client
            .lock()
            .await
            .url_is_in_my_tenant_domain(config_download_request.download_info.url())
        {
            let token = self.http_client.lock().await.get_jwt_token().await?;
            config_download_request.download_info.auth = Some(Auth::new_bearer(&token.token()));
        }

        // Download a file to tmp dir
        let downloader = config_download_request.create_downloader();
        downloader
            .download(&config_download_request.download_info)
            .await?;

        // Move the downloaded file to the final destination
        config_download_request.move_file()?;

        Ok(())
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConfigDownloadRequest {
    pub download_info: DownloadInfo,
    pub file_path: PathBuf,
    pub tmp_dir: PathBuf,
    pub file_permissions: PermissionEntry,
    pub file_name: String,
}

impl ConfigDownloadRequest {
    fn try_new(
        download_url: &str,
        file_path: PathBuf,
        tmp_dir: PathBuf,
        file_permissions: PermissionEntry,
    ) -> Result<Self, ConfigManagementError> {
        let file_name = get_filename(file_path.clone()).ok_or_else(|| {
            ConfigManagementError::FileNameNotFound {
                path: file_path.clone(),
            }
        })?;

        Ok(Self {
            download_info: DownloadInfo {
                url: download_url.into(),
                auth: None,
            },
            file_path,
            tmp_dir,
            file_permissions,
            file_name,
        })
    }

    fn has_write_access(&self) -> Result<(), ConfigManagementError> {
        let metadata =
            if self.file_path.is_file() {
                get_metadata(&self.file_path)?
            } else {
                // If the file does not exist before downloading file, check the directory perms
                let parent_dir = &self.file_path.parent().ok_or_else(|| {
                    ConfigManagementError::NoWriteAccess {
                        path: self.file_path.clone(),
                    }
                })?;
                get_metadata(parent_dir)?
            };

        // Write permission check
        if metadata.permissions().readonly() {
            Err(ConfigManagementError::NoWriteAccess {
                path: self.file_path.clone(),
            })
        } else {
            Ok(())
        }
    }

    fn create_downloader(&self) -> Downloader {
        Downloader::new(&self.file_name, &None, &self.tmp_dir)
    }

    fn move_file(&self) -> Result<(), ConfigManagementError> {
        let src = &self.tmp_dir.join(&self.file_name);
        let dest = &self.file_path;

        if let Some(dest_dir) = dest.parent() {
            if !dest_dir.exists() {
                fs::create_dir_all(dest_dir)?;
            }
        }

        let original_permission_mode = match self.file_path.is_file() {
            true => {
                let metadata = get_metadata(&self.file_path)?;
                let mode = metadata.permissions().mode();
                Some(mode)
            }
            false => None,
        };

        let _ = fs::copy(src, dest).map_err(|_| ConfigManagementError::FileCopyFailed {
            src: src.to_path_buf(),
            dest: dest.to_path_buf(),
        })?;

        let file_permissions = if let Some(mode) = original_permission_mode {
            // Use the same file permission as the original one
            PermissionEntry::new(None, None, Some(mode))
        } else {
            // Set the user, group, and mode as given for a new file
            self.file_permissions.clone()
        };

        file_permissions.apply(&self.file_path)?;

        Ok(())
    }
}

pub fn get_file_change_notification_message(file_path: &str, config_type: &str) -> Message {
    let notification = json!({ "path": file_path }).to_string();
    let topic = Topic::new(format!("{CONFIG_CHANGE_TOPIC}/{config_type}").as_str())
        .unwrap_or_else(|_err| {
            warn!("The type cannot be used as a part of the topic name. Using {CONFIG_CHANGE_TOPIC} instead.");
            Topic::new_unchecked(CONFIG_CHANGE_TOPIC)
        });
    Message::new(&topic, notification)
}

pub struct DownloadConfigFileStatusMessage {}

impl TryIntoOperationStatusMessage for DownloadConfigFileStatusMessage {
    fn status_executing() -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yDownloadConfigFile)
            .to_smartrest()
    }

    fn status_successful(
        _parameter: Option<String>,
    ) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yDownloadConfigFile)
            .to_smartrest()
    }

    fn status_failed(failure_reason: String) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yDownloadConfigFile,
            failure_reason,
        )
        .to_smartrest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::*;

    #[test]
    fn create_config_download_request() -> Result<(), anyhow::Error> {
        let config_download_request = ConfigDownloadRequest::try_new(
            "https://test.cumulocity.com/inventory/binaries/70208",
            PathBuf::from("/etc/tedge/tedge.toml"),
            PathBuf::from("/tmp"),
            PermissionEntry::default(),
        )?;

        assert_eq!(
            config_download_request,
            ConfigDownloadRequest {
                download_info: DownloadInfo {
                    url: "https://test.cumulocity.com/inventory/binaries/70208".to_string(),
                    auth: None
                },
                file_path: PathBuf::from("/etc/tedge/tedge.toml"),
                tmp_dir: PathBuf::from("/tmp"),
                file_permissions: PermissionEntry::new(None, None, None),
                file_name: "tedge.toml".to_string()
            }
        );
        Ok(())
    }

    #[test]
    fn create_config_download_request_without_file_name() -> Result<(), anyhow::Error> {
        let error = ConfigDownloadRequest::try_new(
            "https://test.cumulocity.com/inventory/binaries/70208",
            PathBuf::from("/"),
            PathBuf::from("/tmp"),
            PermissionEntry::default(),
        )
        .unwrap_err();

        assert_matches!(error, ConfigManagementError::FileNameNotFound { .. });
        Ok(())
    }

    #[test]
    fn get_smartrest_executing() {
        let message = DownloadConfigFileStatusMessage::executing().unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "501,c8y_DownloadConfigFile\n"
        );
    }

    #[test]
    fn get_smartrest_successful() {
        let message = DownloadConfigFileStatusMessage::successful(None).unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "503,c8y_DownloadConfigFile,\n"
        );
    }

    #[test]
    fn get_smartrest_failed() {
        let message = DownloadConfigFileStatusMessage::failed("failed reason".to_string()).unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "502,c8y_DownloadConfigFile,\"failed reason\"\n"
        );
    }
}
