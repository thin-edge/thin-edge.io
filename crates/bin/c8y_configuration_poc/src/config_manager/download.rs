use super::config_manager::ActiveOperationState;
use super::config_manager::CONFIG_CHANGE_TOPIC;
use super::config_manager::DEFAULT_OPERATION_DIR_NAME;
use super::config_manager::DEFAULT_OPERATION_TIMEOUT;
use super::config_manager::DEFAULT_PLUGIN_CONFIG_FILE_NAME;
use super::error;
use super::error::ChildDeviceConfigManagementError;
use super::error::ConfigManagementError;
use super::plugin_config::FileEntry;
use super::plugin_config::PluginConfig;
use super::ConfigManagerConfig;
use crate::c8y_http_proxy::messages::C8YRestRequest;
use crate::c8y_http_proxy::messages::C8YRestResponse;
use crate::mqtt_ext::MqttMessage;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use c8y_api::smartrest::topic::C8yTopic;
use download::Auth;
use download::DownloadInfo;
use download::Downloader;
use mqtt_channel::Message;
use mqtt_channel::SinkExt;
use mqtt_channel::Topic;
use mqtt_channel::UnboundedSender;
use tedge_actors::mpsc;
use tedge_actors::DynSender;
use tedge_api::OperationStatus;

use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tedge_utils::file::get_filename;
use tedge_utils::file::get_metadata;
use tedge_utils::file::PermissionEntry;
use tedge_utils::timers::Timers;
use tokio::sync::Mutex;
use tracing::error;
use tracing::info;
use tracing::warn;

pub struct ConfigDownloadManager {
    config: ConfigManagerConfig,
    mqtt_publisher: DynSender<MqttMessage>,
    c8y_http_req_sender: DynSender<C8YRestRequest>,
    c8y_http_res_receiver: mpsc::Receiver<C8YRestResponse>,
    pub operation_timer: Timers<(String, String), ActiveOperationState>,
}

impl ConfigDownloadManager {
    pub fn new(
        config: ConfigManagerConfig,
        mqtt_publisher: DynSender<MqttMessage>,
        c8y_http_req_sender: DynSender<C8YRestRequest>,
        c8y_http_res_receiver: mpsc::Receiver<C8YRestResponse>,
    ) -> Self {
        ConfigDownloadManager {
            config,
            mqtt_publisher,
            c8y_http_req_sender,
            c8y_http_res_receiver,
            operation_timer: Timers::new(),
        }
    }

    pub async fn handle_config_download_request(
        &mut self,
        smartrest_request: SmartRestConfigDownloadRequest,
    ) -> Result<(), anyhow::Error> {
        info!(
            "Received c8y_DownloadConfigFile request for config type: {} from device: {}",
            smartrest_request.config_type, smartrest_request.device
        );

        self.handle_config_download_request_tedge_device(smartrest_request)
            .await
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
