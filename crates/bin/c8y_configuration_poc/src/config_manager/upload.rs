use super::actor::ConfigManagerMessageBox;
use super::plugin_config::PluginConfig;
use super::ConfigManagerConfig;
use anyhow::Result;
use c8y_api::smartrest::error::SmartRestSerializerError;
use c8y_api::smartrest::smartrest_deserializer::SmartRestConfigUploadRequest;
use c8y_api::smartrest::smartrest_serializer::CumulocitySupportedOperations;
use c8y_api::smartrest::smartrest_serializer::SmartRest;
use c8y_api::smartrest::smartrest_serializer::SmartRestSerializer;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToExecuting;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToFailed;
use c8y_api::smartrest::smartrest_serializer::SmartRestSetOperationToSuccessful;
use c8y_api::smartrest::smartrest_serializer::TryIntoOperationStatusMessage;
use std::path::Path;
use tracing::error;
use tracing::info;

pub struct ConfigUploadManager {
    config: ConfigManagerConfig,
}

impl ConfigUploadManager {
    pub fn new(config: ConfigManagerConfig) -> Self {
        ConfigUploadManager { config }
    }

    pub async fn handle_config_upload_request(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<()> {
        info!(
            "Received c8y_UploadConfigFile request for config type: {} from device: {}",
            config_upload_request.config_type, config_upload_request.device
        );

        self.handle_config_upload_request_tedge_device(config_upload_request, message_box)
            .await
    }

    pub async fn handle_config_upload_request_tedge_device(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<()> {
        // set config upload request to executing
        let msg = UploadConfigFileStatusMessage::executing()?;
        message_box.mqtt_publisher.send(msg).await?;

        let plugin_config = PluginConfig::new(&self.config.plugin_config_path);

        let upload_result = {
            match plugin_config.get_file_entry_from_type(&config_upload_request.config_type) {
                Ok(file_entry) => {
                    let config_file_path = file_entry.path;
                    self.upload_config_file(
                        Path::new(config_file_path.as_str()),
                        &config_upload_request.config_type,
                        message_box,
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
                message_box.mqtt_publisher.send(successful_message).await?;
            }
            Err(err) => {
                error!("The configuration upload for '{target_config_type}' failed.",);

                let failed_message = UploadConfigFileStatusMessage::failed(err.to_string())?;
                message_box.mqtt_publisher.send(failed_message).await?;
            }
        }

        Ok(())
    }

    pub async fn upload_config_file(
        &mut self,
        config_file_path: &Path,
        config_type: &str,
        message_box: &mut ConfigManagerMessageBox,
    ) -> Result<String> {
        let url = message_box
            .c8y_http_proxy
            .upload_config_file(config_file_path, config_type, None)
            .await
            .unwrap();
        Ok(url)
    }
}

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
