use crate::{
    child_device::{ConfigOperationRequest, ConfigOperationResponse},
    PluginConfig, DEFAULT_PLUGIN_CONFIG_FILE_NAME,
};
use agent_interface::{DownloadInfo, Downloader, OperationStatus};
use anyhow::Result;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_serializer::{SmartRest, TryIntoOperationStatusMessage};
use c8y_api::smartrest::{
    smartrest_deserializer::SmartRestConfigUploadRequest,
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
    },
};

use mqtt_channel::{Connection, Message, SinkExt, Topic};
use reqwest::Client;
use tedge_utils::file::{create_directory_with_user_group, create_file_with_user_group};

use std::path::Path;
use tracing::{debug, error, info};

pub struct UploadConfigFileStatusMessage {}

impl TryIntoOperationStatusMessage for UploadConfigFileStatusMessage {
    // returns a c8y message specifying to set the upload config file operation status to executing.
    // example message: '501,c8y_UploadConfigFile'
    fn status_executing() -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToExecuting::new(CumulocitySupportedOperations::C8yUploadConfigFile)
            .to_smartrest()
    }

    // returns a c8y SmartREST message indicating the success of the upload config file operation.
    // example message: '503,c8y_UploadConfigFile,https://{c8y.url}/etc...'
    fn status_successful(parameter: Option<String>) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToSuccessful::new(CumulocitySupportedOperations::C8yUploadConfigFile)
            .with_response_parameter(parameter.unwrap_or_else(|| "".to_string()).as_str())
            .to_smartrest()
    }

    // returns a c8y SmartREST message indicating the failure of the upload config file operation.
    // example message: '502,c8y_UploadConfigFile,"failure reason"'
    fn status_failed(failure_reason: String) -> Result<SmartRest, SmartRestSerializerError> {
        SmartRestSetOperationToFailed::new(
            CumulocitySupportedOperations::C8yUploadConfigFile,
            failure_reason,
        )
        .to_smartrest()
    }
}

pub async fn handle_config_upload_request(
    config_upload_request: SmartRestConfigUploadRequest,
    mqtt_client: &mut Connection,
    http_client: &mut impl C8YHttpProxy,
    local_http_host: &str,
    tedge_device_id: &str,
    config_dir: &Path,
) -> Result<()> {
    if config_upload_request.device == tedge_device_id {
        handle_config_upload_request_tedge_device(
            config_upload_request,
            mqtt_client,
            http_client,
            config_dir,
        )
        .await
    } else {
        handle_config_upload_request_child_device(
            config_upload_request,
            mqtt_client,
            local_http_host,
            config_dir,
        )
        .await
    }
}

pub async fn handle_config_upload_request_tedge_device(
    config_upload_request: SmartRestConfigUploadRequest,
    mqtt_client: &mut Connection,
    http_client: &mut impl C8YHttpProxy,
    config_dir: &Path,
) -> Result<()> {
    // set config upload request to executing
    let msg = UploadConfigFileStatusMessage::executing()?;
    mqtt_client.published.send(msg).await?;

    let config_file_path = config_dir.join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);
    let plugin_config = PluginConfig::new(&config_file_path);

    let upload_result = {
        match plugin_config.get_file_entry_from_type(&config_upload_request.config_type) {
            Ok(file_entry) => {
                let config_file_path = file_entry.path;
                upload_config_file(
                    Path::new(config_file_path.as_str()),
                    &config_upload_request.config_type,
                    http_client,
                )
                .await
            }
            Err(err) => Err(err.into()),
        }
    };

    let target_config_type = &config_upload_request.config_type;

    match upload_result {
        Ok(upload_event_url) => {
            info!("The configuration upload for '{target_config_type}' is successful.");

            let successful_message =
                UploadConfigFileStatusMessage::successful(Some(upload_event_url))?;
            mqtt_client.published.send(successful_message).await?;
        }
        Err(err) => {
            error!("The configuration upload for '{target_config_type}' failed.",);

            let failed_message = UploadConfigFileStatusMessage::failed(err.to_string())?;
            mqtt_client.published.send(failed_message).await?;
        }
    }

    Ok(())
}

pub async fn upload_config_file(
    config_file_path: &Path,
    config_type: &str,
    http_client: &mut impl C8YHttpProxy,
) -> Result<String> {
    // upload config file
    let upload_event_url = http_client
        .upload_config_file(config_file_path, config_type, None)
        .await?;

    Ok(upload_event_url)
}

/// Map the c8y_UploadConfigFile request into a tedge/commands/req/config_snapshot command for the child device.
/// The child device is expected to upload the config fie snapshot to the file transfer service.
/// A unique URL path for this config file, from the file transfer service, is shared with the child device in the command.
/// The child device can use this URL to upload the config file snapshot to the file transfer service.
pub async fn handle_config_upload_request_child_device(
    config_upload_request: SmartRestConfigUploadRequest,
    mqtt_client: &mut Connection,
    local_http_host: &str,
    config_dir: &Path,
) -> Result<()> {
    let child_id = config_upload_request.device;
    let config_type = config_upload_request.config_type;

    let plugin_config = PluginConfig::new(Path::new(&format!(
        "{}/c8y/{child_id}/c8y-configuration-plugin.toml",
        config_dir.display()
    )));

    let file_entry = plugin_config.get_file_entry_from_type(&config_type)?;

    let config_management = ConfigOperationRequest::Snapshot {
        child_id,
        file_entry,
    };

    info!("Sending config snapshot request to child device");
    let msg = Message::new(
        &config_management.operation_request_topic(),
        config_management.operation_request_payload(local_http_host)?,
    );
    mqtt_client.published.send(msg).await?;

    Ok(())
}

pub async fn handle_child_device_config_snapshot_response(
    message: &Message,
    tmp_dir: &Path,
    http_client: &mut impl C8YHttpProxy,
    local_http_host: &str,
    config_dir: &Path,
) -> Result<Message, anyhow::Error> {
    let config_response = ConfigOperationResponse::try_from(message)?;
    let payload = config_response.get_payload();
    let c8y_child_topic = Topic::new_unchecked(&config_response.get_child_topic());

    if let Some(operation_status) = payload.status {
        match operation_status {
            OperationStatus::Successful => {
                Ok(handle_child_device_successful_config_snapshot_response(
                    &config_response,
                    tmp_dir,
                    http_client,
                    local_http_host,
                )
                .await?)
            }
            OperationStatus::Failed => {
                if let Some(error_message) = &payload.reason {
                    let failed_status_payload =
                        UploadConfigFileStatusMessage::status_failed(error_message.to_string())?;
                    Ok(Message::new(&c8y_child_topic, failed_status_payload))
                } else {
                    let default_error_message =
                        String::from("No fail reason provided by child device.");
                    let failed_status_payload =
                        UploadConfigFileStatusMessage::status_failed(default_error_message)?;
                    Ok(Message::new(&c8y_child_topic, failed_status_payload))
                }
            }
            OperationStatus::Executing => {
                // is cloud request pending?
                let executing_status_payload = UploadConfigFileStatusMessage::status_executing()?;
                Ok(Message::new(&c8y_child_topic, executing_status_payload))
            }
        }
    } else {
        if &config_response.get_config_type() == "c8y-configuration-plugin" {
            // create directories
            create_directory_with_user_group(
                format!(
                    "{}/c8y/{}",
                    config_dir.display(),
                    config_response.get_child_id()
                ),
                "tedge",
                "tedge",
                0o755,
            )?;
            create_directory_with_user_group(
                format!(
                    "{}/operations/c8y/{}",
                    config_dir.display(),
                    config_response.get_child_id()
                ),
                "tedge",
                "tedge",
                0o755,
            )?;
            create_file_with_user_group(
                format!(
                    "{}/operations/c8y/{}/c8y_DownloadConfigFile",
                    config_dir.display(),
                    config_response.get_child_id()
                ),
                "tedge",
                "tedge",
                0o755,
                None,
            )?;
            create_file_with_user_group(
                format!(
                    "{}/operations/c8y/{}/c8y_UploadConfigFile",
                    config_dir.display(),
                    config_response.get_child_id()
                ),
                "tedge",
                "tedge",
                0o755,
                None,
            )?;
            // copy to /etc/c8y
            let path_from = &format!(
                "/var/tedge/file-transfer/{}/c8y-configuration-plugin",
                config_response.get_child_id()
            );
            let path_from = Path::new(path_from);
            let path_to = &format!(
                "{}/c8y/{}/c8y-configuration-plugin.toml",
                config_dir.display(),
                config_response.get_child_id()
            );
            let path_to = Path::new(path_to);
            std::fs::rename(path_from, path_to)?;
        }
        // send 119
        let child_plugin_config = PluginConfig::new(Path::new(&format!(
            "{}/c8y/{}/c8y-configuration-plugin.toml",
            config_dir.display(),
            config_response.get_child_id()
        )));

        // Publish supported configuration types for child devices
        let message = child_plugin_config
            .to_supported_config_types_message_for_child(&config_response.get_child_id())?;
        Ok(message)
    }
}

pub async fn handle_child_device_successful_config_snapshot_response(
    config_response: &ConfigOperationResponse,
    tmp_dir: &Path,
    http_client: &mut impl C8YHttpProxy,
    local_http_host: &str,
) -> Result<Message, anyhow::Error> {
    let c8y_child_topic = Topic::new_unchecked(&config_response.get_child_topic());

    let config_file_url = format!(
        "http://{}/tedge/file-transfer/{}",
        local_http_host,
        config_response.http_file_repository_relative_path()
    );

    let url_data = DownloadInfo::new(config_file_url.as_str());
    let downloader = Downloader::new(&config_response.get_config_type(), &None, tmp_dir);
    info!("Downloading the config file snapshot uploaded by the child device from url: {config_file_url}");
    downloader.download(&url_data).await?;

    info!(
        "Uploading the downloaded config file snapshot at {:?} to Cumulocity",
        downloader.filename()
    );
    let c8y_upload_event_url = http_client
        .upload_config_file(
            downloader.filename(),
            &config_response.get_config_type(),
            Some(config_response.get_child_id()),
        )
        .await?;

    info!("Marking the c8y_UploadConfigFile operation as successful with the Cumulocity URL for the uploaded file: {config_file_url}");
    let successful_status_payload =
        UploadConfigFileStatusMessage::status_successful(Some(c8y_upload_event_url))?;
    let message = Message::new(&c8y_child_topic, successful_status_payload);

    debug!("Deleting the config file snapshot of the child device from file transfer service");
    let _response = Client::new().delete(&config_file_url).send().await?;

    debug!(
        "Deleting the config file snapshot temporary copy downloaded from file transfer service"
    );
    downloader.cleanup().await?;
    Ok(message)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use c8y_api::http_proxy::MockC8YHttpProxy;
    use c8y_api::smartrest::topic::C8yTopic;
    use mockall::predicate;
    use mqtt_channel::Topic;
    use tedge_test_utils::fs::TempTedgeDir;

    const TEST_TIMEOUT_MS: Duration = Duration::from_millis(5000);

    #[test]
    fn get_smartrest_executing() {
        let message = UploadConfigFileStatusMessage::executing().unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(message.payload_str().unwrap(), "501,c8y_UploadConfigFile\n");
    }

    #[test]
    fn get_smartrest_successful() {
        let message =
            UploadConfigFileStatusMessage::successful(Some("https://{c8y.url}/etc".to_string()))
                .unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "503,c8y_UploadConfigFile,https://{c8y.url}/etc\n"
        );
    }

    #[test]
    fn get_smartrest_failed() {
        let message = UploadConfigFileStatusMessage::failed("failed reason".to_string()).unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "502,c8y_UploadConfigFile,\"failed reason\"\n"
        );
    }

    #[tokio::test]
    async fn test_upload_config_file() -> anyhow::Result<()> {
        let config_path = Path::new("/some/temp/path");
        let config_type = "config_type";

        let mut http_client = MockC8YHttpProxy::new();

        http_client
            .expect_upload_config_file()
            .with(
                predicate::eq(config_path),
                predicate::eq(config_type),
                predicate::eq(None),
            )
            .return_once(|_path, _type, _child_id| Ok("http://server/config/file/url".to_string()));

        assert_eq!(
            upload_config_file(config_path, config_type, &mut http_client).await?,
            "http://server/config/file/url"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[serial_test::serial]
    async fn test_handle_config_upload_request() -> anyhow::Result<()> {
        let tedge_device_id = "tedge-device";
        let config_path = Path::new("/some/test/config");
        let ttd = TempTedgeDir::new();
        ttd.dir("c8y")
            .file("c8y-configuration-plugin.toml")
            .with_toml_content(toml::toml! {
                files = [
                    { path = "/some/test/config", type = "config_type" }
                ]
            });

        let broker = mqtt_tests::test_mqtt_broker();
        let mqtt_config = mqtt_channel::Config::default()
            .with_port(broker.port)
            .with_subscriptions(mqtt_channel::TopicFilter::new_unchecked(
                &C8yTopic::SmartRestRequest.to_string(),
            ));
        let mut mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;

        let mut messages = broker.messages_published_on("c8y/s/us").await;

        let mut http_client = MockC8YHttpProxy::new();
        http_client
            .expect_upload_config_file()
            .with(
                predicate::eq(config_path),
                predicate::eq("config_type"),
                predicate::eq(None),
            )
            .return_once(|_path, _type, _child_id| Ok("http://server/config/file/url".to_string()));

        let config_upload_request = SmartRestConfigUploadRequest {
            message_id: "526".to_string(),
            device: tedge_device_id.to_string(),
            config_type: "config_type".to_string(),
        };

        tokio::spawn(async move {
            let _ = handle_config_upload_request(
                config_upload_request,
                &mut mqtt_client,
                &mut http_client,
                "".into(),
                tedge_device_id,
                &ttd.path().join("c8y"),
            )
            .await;
        });

        // Assert the c8y_UploadConfigFile operation transitioning from EXECUTING(501) to SUCCESSFUL(503) with the uploaded config URL
        mqtt_tests::assert_received_all_expected(
            &mut messages,
            TEST_TIMEOUT_MS,
            &[
                "501,c8y_UploadConfigFile",
                "503,c8y_UploadConfigFile,http://server/config/file/url",
            ],
        )
        .await;

        Ok(())
    }
}
