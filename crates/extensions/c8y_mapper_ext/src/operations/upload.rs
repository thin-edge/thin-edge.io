use c8y_http_proxy::messages::CreateEvent;
use camino::Utf8Path;
use mime::Mime;
use std::collections::HashMap;
use tedge_api::{
    entity_store::EntityExternalId, mqtt_topics::OperationType, workflow::GenericCommandState,
};
use tedge_config::AutoLogUpload;
use tedge_mqtt_ext::MqttMessage;
use tedge_uploader_ext::{ContentType, FormData, UploadRequest};
use time::OffsetDateTime;
use tracing::error;
use url::Url;

use crate::{converter::UploadOperationLog, error::ConversionError};

use super::OperationHandler;

impl OperationHandler {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn upload_file(
        &self,
        external_id: &EntityExternalId,
        file_path: &Utf8Path,
        file_name: Option<String>,
        mime_type: Option<Mime>,
        cmd_id: &str,
        event_type: String,
        event_text: Option<String>,
    ) -> Result<Url, ConversionError> {
        let create_event = CreateEvent {
            event_type: event_type.clone(),
            time: OffsetDateTime::now_utc(),
            text: event_text.unwrap_or(event_type),
            extras: HashMap::new(),
            device_id: external_id.into(),
        };

        let event_response_id = self
            .http_proxy
            .lock()
            .await
            .send_event(create_event)
            .await?;

        let binary_upload_event_url = self
            .c8y_endpoint
            .get_url_for_event_binary_upload_unchecked(&event_response_id);

        let proxy_url = self.auth_proxy.proxy_url(binary_upload_event_url.clone());

        let external_id = external_id.as_ref();
        let file_name = file_name.unwrap_or_else(|| {
            format!(
                "{external_id}_{filename}",
                filename = file_path.file_name().unwrap_or(cmd_id)
            )
        });
        let form_data = if let Some(mime) = mime_type {
            FormData::new(file_name).set_mime(mime)
        } else {
            FormData::new(file_name)
        };
        // The method must be POST, otherwise file name won't be supported.
        let upload_request = UploadRequest::new(proxy_url.as_str(), file_path)
            .post()
            .with_content_type(ContentType::FormData(form_data));

        self.uploader_sender
            .send((cmd_id.into(), upload_request))
            .await?;

        Ok(binary_upload_event_url)
    }

    pub async fn upload_operation_log(
        &self,
        external_id: &EntityExternalId,
        cmd_id: &str,
        op_type: &OperationType,
        command: GenericCommandState,
        final_messages: Vec<MqttMessage>,
    ) -> Vec<MqttMessage> {
        if command.is_finished()
            && command.get_log_path().is_some()
            && (self.auto_log_upload == AutoLogUpload::Always
                || (self.auto_log_upload == AutoLogUpload::OnFailure && command.is_failed()))
        {
            let log_path = command.get_log_path().unwrap();
            let event_type = format!("{}_op_log", op_type);
            let event_text = format!("{} operation log", &op_type);
            match self
                .upload_file(
                    external_id,
                    &log_path,
                    None,
                    None,
                    cmd_id,
                    event_type,
                    Some(event_text),
                )
                .await
            {
                Ok(_) => {
                    self.pending_upload_operations
                        .lock()
                        .await
                        .insert(cmd_id.into(), UploadOperationLog { final_messages }.into());
                    return vec![];
                }
                Err(err) => {
                    error!("Operation log upload failed due to {}", err);
                }
            }
        }
        final_messages
    }
}
