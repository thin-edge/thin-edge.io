use crate::config::FileEntry;
use crate::error::ConfigManagementError;
use crate::{error, PluginConfig, CONFIG_CHANGE_TOPIC};
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_smartrest::error::SmartRestSerializerError;
use c8y_smartrest::smartrest_deserializer::SmartRestConfigDownloadRequest;
use c8y_smartrest::smartrest_serializer::{
    CumulocitySupportedOperations, SmartRest, SmartRestSerializer,
    SmartRestSetOperationToExecuting, SmartRestSetOperationToFailed,
    SmartRestSetOperationToSuccessful, TryIntoOperationStatusMessage,
};
use download::{Auth, DownloadInfo, Downloader};
use mqtt_channel::{Connection, Message, SinkExt, Topic};
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tedge_utils::file::{get_filename, get_metadata, PermissionEntry};
use tracing::{info, warn};

pub async fn handle_config_download_request(
    plugin_config: &PluginConfig,
    smartrest_request: SmartRestConfigDownloadRequest,
    tmp_dir: PathBuf,
    mqtt_client: &mut Connection,
    http_client: &mut impl C8YHttpProxy,
) -> Result<(), anyhow::Error> {
    let executing_message = DownloadConfigFileStatusMessage::executing()?;
    let () = mqtt_client.published.send(executing_message).await?;

    let target_config_type = smartrest_request.config_type.clone();
    let mut target_file_entry = FileEntry::default();

    let download_result = {
        match plugin_config.get_file_entry_from_type(&target_config_type) {
            Ok(file_entry) => {
                target_file_entry = file_entry;
                download_config_file(
                    smartrest_request.url.as_str(),
                    PathBuf::from(&target_file_entry.path),
                    tmp_dir,
                    target_file_entry.file_permissions,
                    http_client,
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
            let () = mqtt_client.published.send(successful_message).await?;

            let notification_message =
                get_file_change_notification_message(&target_file_entry.path, &target_config_type);
            let () = mqtt_client.published.send(notification_message).await?;
            Ok(())
        }
        Err(err) => {
            error!("The configuration download for '{target_config_type}' failed.",);

            let failed_message = DownloadConfigFileStatusMessage::failed(err.to_string())?;
            let () = mqtt_client.published.send(failed_message).await?;
            Err(err)
        }
    }
}

async fn download_config_file(
    download_url: &str,
    file_path: PathBuf,
    tmp_dir: PathBuf,
    file_permissions: PermissionEntry,
    http_client: &mut impl C8YHttpProxy,
) -> Result<(), anyhow::Error> {
    // Convert smartrest request to config download request struct
    let mut config_download_request =
        ConfigDownloadRequest::try_new(download_url, file_path, tmp_dir, file_permissions)?;

    // Confirm that the file has write access before any http request attempt
    let () = config_download_request.has_write_access()?;

    // If the provided url is c8y, add auth
    if http_client.url_is_in_my_tenant_domain(config_download_request.download_info.url()) {
        let token = http_client.get_jwt_token().await?;
        config_download_request.download_info.auth = Some(Auth::new_bearer(&token.token()));
    }

    // Download a file to tmp dir
    let downloader = config_download_request.create_downloader();
    let () = downloader
        .download(&config_download_request.download_info)
        .await?;

    // Move the downloaded file to the final destination
    let () = config_download_request.move_file()?;

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
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

        let () = file_permissions.apply(&self.file_path)?;

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

struct DownloadConfigFileStatusMessage {}

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
