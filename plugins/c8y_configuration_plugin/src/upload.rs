use crate::PluginConfig;
use anyhow::Result;
use c8y_api::http_proxy::C8YHttpProxy;
use c8y_smartrest::error::SmartRestSerializerError;
use c8y_smartrest::smartrest_serializer::{SmartRest, TryIntoOperationStatusMessage};
use c8y_smartrest::{
    smartrest_deserializer::SmartRestConfigUploadRequest,
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
    },
};
use mqtt_channel::{Connection, SinkExt};
use std::path::Path;
use tracing::{error, info};

struct UploadConfigFileStatusMessage {}

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
    plugin_config: &PluginConfig,
    config_upload_request: SmartRestConfigUploadRequest,
    mqtt_client: &mut Connection,
    http_client: &mut impl C8YHttpProxy,
) -> Result<()> {
    // set config upload request to executing
    let msg = UploadConfigFileStatusMessage::executing()?;
    let () = mqtt_client.published.send(msg).await?;

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
            let () = mqtt_client.published.send(successful_message).await?;
        }
        Err(err) => {
            error!("The configuration upload for '{target_config_type}' failed.",);

            let failed_message = UploadConfigFileStatusMessage::failed(err.to_string())?;
            let () = mqtt_client.published.send(failed_message).await?;
        }
    }

    Ok(())
}

async fn upload_config_file(
    config_file_path: &Path,
    config_type: &str,
    http_client: &mut impl C8YHttpProxy,
) -> Result<String> {
    // upload config file
    let upload_event_url = http_client
        .upload_config_file(config_file_path, config_type)
        .await?;

    Ok(upload_event_url)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use super::*;
    use crate::config::FileEntry;
    use c8y_api::http_proxy::MockC8YHttpProxy;
    use c8y_smartrest::topic::C8yTopic;
    use mockall::predicate;
    use mqtt_channel::Topic;

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
            .with(predicate::eq(config_path), predicate::eq(config_type))
            .return_once(|_path, _type| Ok("http://server/config/file/url".to_string()));

        assert_eq!(
            upload_config_file(config_path, config_type, &mut http_client).await?,
            "http://server/config/file/url"
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_handle_config_upload_request() -> anyhow::Result<()> {
        let config_path = Path::new("/some/test/config");

        let broker = mqtt_tests::test_mqtt_server::MqttProcessHandler::new(55700, 3700);
        let mqtt_config = mqtt_channel::Config::default()
            .with_port(broker.port)
            .with_subscriptions(mqtt_channel::TopicFilter::new_unchecked(
                C8yTopic::SmartRestRequest.as_str(),
            ));
        let mut mqtt_client = mqtt_channel::Connection::new(&mqtt_config).await?;

        let mut messages = broker.messages_published_on("c8y/s/us").await;

        let mut http_client = MockC8YHttpProxy::new();
        http_client
            .expect_upload_config_file()
            .with(predicate::eq(config_path), predicate::eq("config_type"))
            .return_once(|_path, _type| Ok("http://server/config/file/url".to_string()));

        let config_upload_request = SmartRestConfigUploadRequest {
            message_id: "526".to_string(),
            device: "thin-edge-device".to_string(),
            config_type: "config_type".to_string(),
        };

        let plugin_config = PluginConfig {
            files: HashSet::from([FileEntry::new_with_path_and_type(
                "/some/test/config".to_string(),
                "config_type".to_string(),
            )]),
        };

        tokio::spawn(async move {
            let _ = handle_config_upload_request(
                &plugin_config,
                config_upload_request,
                &mut mqtt_client,
                &mut http_client,
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
