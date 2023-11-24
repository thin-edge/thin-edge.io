use crate::smartrest::smartrest_deserializer::to_datetime;
use mqtt_channel::Topic;
use serde::Deserialize;
use time::OffsetDateTime;

pub struct C8yDeviceControlTopic;
impl C8yDeviceControlTopic {
    pub fn topic() -> Topic {
        Topic::new_unchecked(Self::name())
    }

    pub fn accept(topic: &Topic) -> bool {
        topic.name.starts_with(Self::name())
    }

    pub fn name() -> &'static str {
        "c8y/devicecontrol/notifications"
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct C8yOperation {
    // externalSource field
    pub external_source: ExternalSource,

    // Operation ID
    pub id: String,

    // Operation status
    pub status: C8yOperationStatus,

    #[serde(flatten)]
    pub extras: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ExternalSource {
    pub external_id: String,
    #[serde(rename = "type")]
    pub source_type: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub enum C8yOperationStatus {
    Pending,
    Executing,
    Successful,
    Failed,
}

/// Representation of c8y_LogfileRequest JSON
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8yLogfileRequest {
    pub search_text: Option<String>,
    pub log_file: String,
    #[serde(deserialize_with = "to_datetime")]
    pub date_to: OffsetDateTime,
    #[serde(deserialize_with = "to_datetime")]
    pub date_from: OffsetDateTime,
    pub maximum_lines: usize,
}

/// Representation of c8y_UploadConfigFile JSON
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8yUploadConfigFile {
    #[serde(rename = "type")]
    pub config_type: String,
}
