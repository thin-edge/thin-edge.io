use crate::command::Command;
use crate::log::MaybeFancy;
use anyhow::Error;
use c8y_api::http_proxy::C8yEndPoint;
use c8y_api::json_c8y::C8yCreateEvent;
use c8y_api::json_c8y::C8yEventResponse;
use c8y_api::json_c8y::C8yManagedObject;
use c8y_api::json_c8y::InternalIdResponse;
use c8y_api::OffsetDateTime;
use certificate::CloudRootCerts;
use reqwest::blocking;
use reqwest::blocking::multipart;
use reqwest::Identity;
use std::collections::HashMap;
use std::path::PathBuf;

/// Upload a file to Cumulocity
pub struct C8yUpload {
    /// TLS Client configuration
    pub identity: Option<Identity>,
    pub cloud_root_certs: CloudRootCerts,

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
        let internal_id = self.get_internal_id()?;
        let event_id = self.create_event(internal_id)?;
        self.upload_file(&event_id)?;

        println!("{event_id}");
        Ok(())
    }
}

impl C8yUpload {
    fn client(&self) -> Result<blocking::Client, Error> {
        let builder = self.cloud_root_certs.blocking_client_builder();
        let builder = if let Some(identity) = &self.identity {
            builder.identity(identity.clone())
        } else {
            builder
        };
        Ok(builder.build()?)
    }

    pub fn get_internal_id(&self) -> Result<String, Error> {
        let url_get_id: String = self.c8y.proxy_url_for_internal_id(&self.device_id);
        let http_result = self.client()?.get(url_get_id).send()?;
        let http_response = http_result.error_for_status()?;
        let object: InternalIdResponse = http_response.json()?;
        Ok(object.id())
    }

    pub fn create_event(&self, internal_id: String) -> Result<String, Error> {
        let c8y_event = C8yCreateEvent {
            source: Some(C8yManagedObject { id: internal_id }),
            event_type: self.event_type.clone(),
            time: OffsetDateTime::now_utc(),
            text: self.text.clone(),
            extras: self.json.clone(),
        };
        let create_event_url = self.c8y.proxy_url_for_create_event();
        let http_result = self
            .client()?
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

        let http_result = self
            .client()?
            .post(upload_file_url)
            .header("Accept", "application/json")
            .header("Content-Type", "multipart/form-data")
            .multipart(form)
            .send()?;
        let _ = http_result.error_for_status()?;
        Ok(())
    }
}
