use crate::smartrest::GetSmartRestMessage;
use anyhow::Result;
use c8y_api::http_proxy::{C8YHttpProxy, JwtAuthHttpProxy};
use c8y_smartrest::error::SmartRestSerializerError;
use c8y_smartrest::smartrest_serializer::SmartRest;
use c8y_smartrest::{
    smartrest_deserializer::SmartRestConfigUploadRequest,
    smartrest_serializer::{
        CumulocitySupportedOperations, SmartRestSerializer, SmartRestSetOperationToExecuting,
        SmartRestSetOperationToFailed, SmartRestSetOperationToSuccessful,
    },
};
use mqtt_channel::{Connection, SinkExt};
use std::{fs::read_to_string, path::Path};

struct GetUploadConfigFileMessage {}

impl GetSmartRestMessage for GetUploadConfigFileMessage {
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
    // example message: '503,c8y_UploadConfigFile,https://{c8y.url}/etc...'
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
    http_client: &mut JwtAuthHttpProxy,
) -> Result<()> {
    // set config upload request to executing
    let msg = GetUploadConfigFileMessage::executing()?;
    let () = mqtt_client.published.send(msg).await?;

    let upload_result = upload_config_file(
        Path::new(config_upload_request.config_type.as_str()),
        http_client,
    )
    .await;
    match upload_result {
        Ok(upload_event_url) => {
            let successful_message =
                GetUploadConfigFileMessage::successful(Some(upload_event_url))?;
            let () = mqtt_client.published.send(successful_message).await?;
        }
        Err(err) => {
            let failed_message = GetUploadConfigFileMessage::failed(err.to_string())?;
            let () = mqtt_client.published.send(failed_message).await?;
        }
    }

    Ok(())
}

async fn upload_config_file(
    config_file_path: &Path,
    http_client: &mut JwtAuthHttpProxy,
) -> Result<String> {
    // read the config file contents
    let config_content = read_to_string(config_file_path)?;

    // upload config file
    let upload_event_url = http_client
        .upload_config_file(config_file_path, &config_content)
        .await?;

    Ok(upload_event_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mqtt_channel::Topic;

    #[test]
    fn get_smartrest_executing() {
        let message = GetUploadConfigFileMessage::executing().unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(message.payload_str().unwrap(), "501,c8y_UploadConfigFile\n");
    }

    #[test]
    fn get_smartrest_successful() {
        let message =
            GetUploadConfigFileMessage::successful(Some("https://{c8y.url}/etc".to_string()))
                .unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "503,c8y_UploadConfigFile,https://{c8y.url}/etc\n"
        );
    }

    #[test]
    fn get_smartrest_failed() {
        let message = GetUploadConfigFileMessage::failed("failed reason".to_string()).unwrap();
        assert_eq!(message.topic, Topic::new("c8y/s/us").unwrap());
        assert_eq!(
            message.payload_str().unwrap(),
            "502,c8y_UploadConfigFile,\"failed reason\"\n"
        );
    }
}
