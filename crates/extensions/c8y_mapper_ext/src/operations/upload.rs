use c8y_http_proxy::messages::CreateEvent;
use camino::Utf8Path;
use mime::Mime;
use std::collections::HashMap;
use tedge_api::entity_store::EntityExternalId;
use tedge_api::mqtt_topics::OperationType;
use tedge_api::workflow::GenericCommandState;
use tedge_config::AutoLogUpload;
use tedge_uploader_ext::ContentType;
use tedge_uploader_ext::FormData;
use tedge_uploader_ext::UploadRequest;
use tedge_uploader_ext::UploadResult;
use time::OffsetDateTime;
use url::Url;

use crate::error::ConversionError;

use super::OperationContext;

impl OperationContext {
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
    ) -> Result<(Url, UploadResult), ConversionError> {
        let create_event = CreateEvent {
            event_type: event_type.clone(),
            time: OffsetDateTime::now_utc(),
            text: event_text.unwrap_or(event_type),
            extras: HashMap::new(),
            device_id: external_id.into(),
        };

        let mut c8y_http_proxy = self.http_proxy.clone();
        let event_response_id = c8y_http_proxy.send_event(create_event).await?;

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

        let (_, download_result) = self
            .uploader
            .clone()
            .await_response((cmd_id.into(), upload_request))
            .await?;

        Ok((binary_upload_event_url, download_result))
    }

    pub async fn upload_operation_log(
        &self,
        external_id: &EntityExternalId,
        cmd_id: &str,
        op_type: &OperationType,
        command: &GenericCommandState,
    ) -> Result<(), ConversionError> {
        if command.is_finished()
            && command.get_log_path().is_some()
            && (self.auto_log_upload == AutoLogUpload::Always
                || (self.auto_log_upload == AutoLogUpload::OnFailure && command.is_failed()))
        {
            let log_path = command.get_log_path().unwrap();
            let event_type = format!("{}_op_log", op_type);
            let event_text = format!("{} operation log", &op_type);
            let _upload_url = self
                .upload_file(
                    external_id,
                    &log_path,
                    None,
                    None,
                    cmd_id,
                    event_type,
                    Some(event_text),
                )
                .await?;
        }

        Ok(())
    }
}
