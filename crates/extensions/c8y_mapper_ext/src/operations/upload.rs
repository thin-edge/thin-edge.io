use c8y_http_proxy::messages::CreateEvent;
use camino::Utf8Path;
use mime::Mime;
use std::collections::HashMap;
use tedge_api::entity_store::EntityExternalId;
use tedge_uploader_ext::{ContentType, FormData, UploadRequest};
use time::OffsetDateTime;
use url::Url;

use crate::error::ConversionError;

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
}
