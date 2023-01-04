use crate::c8y_http_proxy::handle::C8YHttpProxy;
use crate::mqtt_ext::MqttMessage;

use super::config_manager::ActiveOperationState;
use super::config_manager::DEFAULT_OPERATION_DIR_NAME;
use super::config_manager::DEFAULT_PLUGIN_CONFIG_FILE_NAME;
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
use tedge_actors::DynSender;
use tedge_utils::timers::Timers;
use tracing::error;
use tracing::info;

pub struct ConfigUploadManager {
    config: ConfigManagerConfig,
    mqtt_publisher: DynSender<MqttMessage>,
    c8y_http_proxy: C8YHttpProxy,
    pub operation_timer: Timers<(String, String), ActiveOperationState>,
}

impl ConfigUploadManager {
    pub fn new(
        config: ConfigManagerConfig,
        mqtt_publisher: DynSender<MqttMessage>,
        c8y_http_proxy: C8YHttpProxy,
    ) -> Self {
        ConfigUploadManager {
            config,
            mqtt_publisher,
            c8y_http_proxy,
            operation_timer: Timers::new(),
        }
    }

    pub async fn handle_config_upload_request(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
    ) -> Result<()> {
        info!(
            "Received c8y_UploadConfigFile request for config type: {} from device: {}",
            config_upload_request.config_type, config_upload_request.device
        );

        self.handle_config_upload_request_tedge_device(config_upload_request)
            .await
    }

    pub async fn handle_config_upload_request_tedge_device(
        &mut self,
        config_upload_request: SmartRestConfigUploadRequest,
    ) -> Result<()> {
        // set config upload request to executing
        let msg = UploadConfigFileStatusMessage::executing()?;
        self.mqtt_publisher.send(msg).await?;

        let config_file_path = self
            .config
            .config_dir
            .join(DEFAULT_OPERATION_DIR_NAME)
            .join(DEFAULT_PLUGIN_CONFIG_FILE_NAME);
        let plugin_config = PluginConfig::new(&config_file_path);

        let upload_result = {
            match plugin_config.get_file_entry_from_type(&config_upload_request.config_type) {
                Ok(file_entry) => {
                    let config_file_path = file_entry.path;
                    self.upload_config_file(
                        Path::new(config_file_path.as_str()),
                        &config_upload_request.config_type,
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
                self.mqtt_publisher.send(successful_message).await?;
            }
            Err(err) => {
                error!("The configuration upload for '{target_config_type}' failed.",);

                let failed_message = UploadConfigFileStatusMessage::failed(err.to_string())?;
                self.mqtt_publisher.send(failed_message).await?;
            }
        }

        Ok(())
    }

    pub async fn upload_config_file(
        &mut self,
        config_file_path: &Path,
        config_type: &str,
    ) -> Result<String> {
        let url = self
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
