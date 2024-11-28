use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::anyhow;
use anyhow::Error;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::C8yManagedObject;
use c8y_api::json_c8y::InternalIdResponse;
use c8y_api::OffsetDateTime;
use reqwest::blocking;
use reqwest::blocking::multipart;
use std::collections::HashMap;
use std::path::PathBuf;

/// Upload a file to Cumulocity
pub struct C8yUpload {
    /// Device identifier
    pub device_id: String,

    /// Cumulocity endpoint
    pub c8y: C8yEndPoint,

    /// Type of the event.
    pub event_type: String,

    /// Text description of the event.
    pub text: String,

    /// JSON fragment attached to the event
    pub json: HashMap<String, serde_json::Value>,

    /// Path to the uploaded file
    pub file: PathBuf,

    /// MIME type of the file content. Defaults to `application/octet-stream`
    pub mime_type: String,
}

impl Command for C8yUpload {
    fn description(&self) -> String {
        "upload a file to Cumulocity".to_string()
    }

    fn execute(&self) -> Result<(), MaybeFancy<Error>> {
        if !self.file.exists() {
            return Err(anyhow!("Failed to open file: {:?}", self.file))?;
        }
        let internal_id = self.get_internal_id()?;
        let event_id = self.create_event(&internal_id)?;
        self.upload_file(&event_id)?;

        println!("{event_id}");
        Ok(())
    }
}

impl C8yUpload {
    pub fn get_internal_id(&self) -> Result<String, Error> {
        let url_get_id: String = self.c8y.proxy_url_for_internal_id(&self.device_id);
        let http_result = blocking::Client::new().get(url_get_id).send()?;
        let http_response = http_result.error_for_status()?;
        let object: InternalIdResponse = http_response.json()?;
        Ok(object.id())
    }

    pub fn create_event(&self, internal_id: &str) -> Result<String, Error> {
        let c8y_event = C8yCreateEvent {
            source: Some(C8yManagedObject {
                id: internal_id.to_string(),
            }),
            event_type: self.event_type.clone(),
            time: OffsetDateTime::now_utc(),
            text: self.text.clone(),
            extras: self.json.clone(),
        };
        let create_event_url = self.c8y.proxy_url_for_create_event();
        let http_result = blocking::Client::new()
            .post(create_event_url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&c8y_event)
            .send()?;
        let http_response = http_result.error_for_status()?;
        let event_response: C8yEventResponse = http_response.json()?;
        Ok(event_response.id)
    }

    pub fn upload_file(&self, event_id: &str) -> Result<(), Error> {
        let upload_file_url = self.c8y.proxy_url_for_event_binary_upload(event_id);
        let mime_type: String = self.mime_type.clone();
        let file = multipart::Part::file(&self.file)?.mime_str(&mime_type)?;
        let form = multipart::Form::new()
            .text("type", mime_type)
            .part("file", file);

        let http_result = blocking::Client::new()
            .post(upload_file_url)
            .header("Accept", "application/json")
            .header("Content-Type", "multipart/form-data")
            .multipart(form)
            .send()?;
        let _ = http_result.error_for_status()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use c8y_api::proxy_url::Protocol;
    use c8y_api::proxy_url::ProxyUrlGenerator;
    use mockito::Matcher;
    use mockito::ServerGuard;
    use serde_json::json;
    use tedge_test_utils::fs::TempTedgeDir;

    #[test]
    fn create_event() {
        let dir = TempTedgeDir::new();
        let file = dir
            .file("uploaded-file.txt")
            .with_raw_content("uploaded-bytes");
        let c8y_event = C8yCreateEvent {
            source: Some(C8yManagedObject {
                id: "internal-test-device".to_string(),
            }),
            event_type: "test".to_string(),
            time: OffsetDateTime::now_utc(),
            text: "hello".to_string(),
            extras: HashMap::default(),
        };

        let c8y = mock_auth_proxy("test-device", "event-123", &c8y_event);
        let upload = upload_cmd(&c8y, file.to_path_buf(), "test-device", c8y_event);

        // Step by step
        assert_eq!(upload.get_internal_id().unwrap(), "internal-test-device");
        assert_eq!(
            upload.create_event("internal-test-device").unwrap(),
            "event-123"
        );
        assert!(upload.upload_file("event-123").is_ok());

        // In one go
        assert!(upload.execute().is_ok());
    }

    fn upload_cmd(
        c8y: &ServerGuard,
        file: PathBuf,
        device_id: &str,
        c8y_event: C8yCreateEvent,
    ) -> C8yUpload {
        let proxy = ProxyUrlGenerator::new(
            "localhost".into(),
            c8y.socket_address().port(),
            Protocol::Http,
        );

        C8yUpload {
            device_id: device_id.to_string(),
            c8y: C8yEndPoint::new("test.c8y.com", "test.c8y.com", proxy),
            event_type: c8y_event.event_type,
            text: c8y_event.text,
            json: Default::default(),
            file,
            mime_type: "text/plain".to_string(),
        }
    }

    fn mock_auth_proxy(device_id: &str, event_id: &str, c8y_event: &C8yCreateEvent) -> ServerGuard {
        let mut c8y = mockito::Server::new();

        // Mock external id requests
        let xid = c8y_event.source.as_ref().unwrap().id.clone();
        c8y.mock(
            "GET",
            format!("/c8y/identity/externalIds/c8y_Serial/{device_id}").as_str(),
        )
        .with_body(
            json!({
                "managedObject": { "id": xid },
                "externalId": device_id,
            })
            .to_string(),
        )
        .with_status(200)
        .create();

        // Mock event creation
        let mut expected_event = serde_json::to_value(c8y_event).unwrap();
        if let Some(event) = expected_event.as_object_mut() {
            event.remove("time");
        }
        c8y.mock("POST", "/c8y/event/events/")
            .match_body(Matcher::PartialJson(expected_event))
            .with_body(json!({ "id": event_id}).to_string())
            .with_status(200)
            .create();

        // Mock file upload
        c8y.mock(
            "POST",
            format!("/c8y/event/events/{event_id}/binaries").as_str(),
        )
        .match_body(Matcher::Regex("uploaded-file.txt".to_string()))
        .with_status(200)
        .create();

        c8y
    }
}
