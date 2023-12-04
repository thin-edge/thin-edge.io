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

#[derive(Debug)]
pub enum C8yDeviceControlOperations {
    Restart,
    SoftwareUpdate,
    LogfileRequest(C8yLogfileRequest),
    UploadConfigFile(C8yUploadConfigFile),
    DownloadConfigFile(C8yDownloadConfigFile),
    Firmware,
}

/// Representation of operation object received via JSON over MQTT
///
/// A lot information come from c8y, however, we only need these items:
/// - `id`, namely c8y's operation ID,
/// - `externalSource.externalId` as device external ID,
/// - operation fragment and its contents, here "c8y_UploadConfigFile".
///
/// ```rust
/// // Example input from c8y
/// use c8y_api::json_c8y_deserializer::{C8yOperation, C8yUploadConfigFile};
///
/// let data = r#"
/// {
///     "delivery": {
///         "log": [],
///         "time": "2023-02-08T06:51:19.350Z",
///         "status": "PENDING"
///     },
///     "agentId": "22519994",
///     "creationTime": "2023-02-08T06:51:19.318Z",
///     "deviceId": "22519994",
///     "id": "522559",
///     "status": "PENDING",
///     "description": "test operation",
///     "c8y_UploadConfigFile": {
///         "type": "/etc/tedge/tedge.toml"
///     },
///     "externalSource": {
///         "externalId": "raspberrypi_001",
///         "type": "c8y_Serial"
///     }
/// }"#;
///
/// // Parse the data
/// let op: C8yOperation = serde_json::from_str(data).unwrap();
///
/// // Get data for processing command
/// let device_xid = op.external_source.external_id;
/// let operation_id = op.op_id;
/// if let Some(v) = op.extras.get("c8y_UploadConfigFile") {
///     let c8y_upload_config_file: C8yUploadConfigFile = serde_json::from_value(v.clone()).unwrap();
/// }
/// ```
#[derive(Debug, Deserialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct C8yOperation {
    /// externalSource
    pub external_source: ExternalSource,

    /// Operation ID
    #[serde(rename = "id")]
    pub op_id: String,

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

/// Representation of c8y_LogfileRequest JSON object
///
/// ```rust
/// use c8y_api::json_c8y_deserializer::C8yLogfileRequest;
///
/// // Example input from c8y
/// let data = r#"
/// {
///     "searchText": "",
///     "logFile": "foobar",
///     "dateTo": "2023-11-22T22:44:34+0100",
///     "dateFrom": "2023-11-21T22:44:34+0100",
///     "maximumLines": 1000
/// }"#;
///
/// // Parse the data
/// let req: C8yLogfileRequest = serde_json::from_str(data).unwrap();
/// ```
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8yLogfileRequest {
    pub search_text: String,
    pub log_file: String,
    #[serde(deserialize_with = "to_datetime")]
    pub date_to: OffsetDateTime,
    #[serde(deserialize_with = "to_datetime")]
    pub date_from: OffsetDateTime,
    pub maximum_lines: usize,
}

/// Representation of c8y_UploadConfigFile JSON object
///
/// ```rust
/// use c8y_api::json_c8y_deserializer::C8yUploadConfigFile;
///
/// // Example input from c8y
/// let data = r#"
/// {
///     "type": "/etc/tedge/tedge.toml"
/// }"#;
///
/// // Parse the data
/// let req: C8yUploadConfigFile = serde_json::from_str(data).unwrap();
/// ```
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8yUploadConfigFile {
    #[serde(rename = "type")]
    pub config_type: String,
}

/// Representation of c8y_DownloadConfigFile JSON object
///
/// ```rust
/// use c8y_api::json_c8y_deserializer::C8yDownloadConfigFile;
///
/// // Example input from c8y
/// let data = r#"
/// {
///     "type": "/etc/tedge/tedge.toml",
///     "url": "https://example.cumulocity.com/inventory/binaries/757538"
/// }"#;
///
/// // Parse the data
/// let req: C8yDownloadConfigFile = serde_json::from_str(data).unwrap();
/// ```
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8yDownloadConfigFile {
    #[serde(rename = "type")]
    pub config_type: String,
    pub url: String,
}
