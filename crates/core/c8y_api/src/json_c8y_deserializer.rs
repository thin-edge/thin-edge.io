use crate::smartrest::smartrest_deserializer::to_datetime;
use download::DownloadInfo;
use mqtt_channel::Topic;
use serde::Deserialize;
use std::collections::HashMap;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::SoftwareModule;
use tedge_api::SoftwareModuleUpdate;
use tedge_api::SoftwareUpdateCommand;
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
    Restart(C8yRestart),
    SoftwareUpdate(C8ySoftwareUpdate),
    LogfileRequest(C8yLogfileRequest),
    UploadConfigFile(C8yUploadConfigFile),
    DownloadConfigFile(C8yDownloadConfigFile),
    Firmware(C8yFirmware),
    Custom,
}

impl C8yDeviceControlOperations {
    pub fn from_extras(
        hashmap: &HashMap<String, serde_json::Value>,
    ) -> Result<Self, serde_json::Error> {
        let op = if let Some(value) = hashmap.get("c8y_Restart") {
            C8yDeviceControlOperations::Restart(C8yRestart::from_value(value.clone())?)
        } else if let Some(value) = hashmap.get("c8y_SoftwareUpdate") {
            C8yDeviceControlOperations::SoftwareUpdate(C8ySoftwareUpdate::from_value(
                value.clone(),
            )?)
        } else if let Some(value) = hashmap.get("c8y_LogfileRequest") {
            C8yDeviceControlOperations::LogfileRequest(C8yLogfileRequest::from_value(
                value.clone(),
            )?)
        } else if let Some(value) = hashmap.get("c8y_UploadConfigFile") {
            C8yDeviceControlOperations::UploadConfigFile(C8yUploadConfigFile::from_value(
                value.clone(),
            )?)
        } else if let Some(value) = hashmap.get("c8y_DownloadConfigFile") {
            C8yDeviceControlOperations::DownloadConfigFile(C8yDownloadConfigFile::from_value(
                value.clone(),
            )?)
        } else if let Some(value) = hashmap.get("c8y_Firmware") {
            C8yDeviceControlOperations::Firmware(C8yFirmware::from_value(value.clone())?)
        } else {
            C8yDeviceControlOperations::Custom
        };

        Ok(op)
    }
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

impl C8yOperation {
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }
}

/// Representation of c8y_Restart JSON object
///
/// ```rust
/// use c8y_api::json_c8y_deserializer::C8yRestart;
///
/// // Example input from c8y
/// let data = "{}";
///
/// // Parse the data
/// let req: C8yRestart = serde_json::from_str(data).unwrap();
/// ```
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct C8yRestart {}

/// Representation of c8y_SoftwareUpdate JSON object
///
/// ```rust
/// use c8y_api::json_c8y_deserializer::{C8ySoftwareUpdate, C8ySoftwareUpdateAction};
///
/// // Example input from c8y
/// let data = r#"[
///     {
///         "softwareType": "dummy",
///         "name": "foo",
///         "action": "install",
///         "id": "123456",
///         "version": "2.0.0",
///         "url": "https://example.cumulocity.com/inventory/binaries/757538"
///     },
///     {
///         "name": "bar",
///         "action": "delete",
///         "version": "1.0.1"
///     }
/// ]"#;
///
/// // Parse the data
/// let req: C8ySoftwareUpdate = serde_json::from_str(data).unwrap();
///
/// let first_list = req.lists.get(0).unwrap();
/// assert_eq!(first_list.software_type, Some("dummy".to_string()));
/// assert_eq!(first_list.name, "foo");
/// assert_eq!(first_list.action, "install");
/// assert_eq!(first_list.version, "2.0.0");
/// assert_eq!(first_list.url, Some("https://example.cumulocity.com/inventory/binaries/757538".to_string()));
///
/// let second_list = req.lists.get(1).unwrap();
/// assert_eq!(second_list.name, "bar");
/// assert_eq!(second_list.action, "delete");
/// assert_eq!(second_list.version, "1.0.1");
/// ```
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(transparent)]
pub struct C8ySoftwareUpdate {
    pub lists: Vec<C8ySoftwareUpdateModule>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8ySoftwareUpdateModule {
    pub name: String,
    pub action: String,
    pub version: String,
    // None if the action is "delete"
    pub url: Option<String>,
    // None if c8y's version is old. See issue #1352
    pub software_type: Option<String>,
    // C8y's object ID of the software to be installed. We don't use this info
    pub id: Option<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum C8ySoftwareUpdateAction {
    Install,
    Delete,
}

impl TryFrom<String> for C8ySoftwareUpdateAction {
    type Error = C8yJsonOverMqttDeserializerError;

    fn try_from(action: String) -> Result<Self, Self::Error> {
        match action.as_str() {
            "install" => Ok(Self::Install),
            "delete" => Ok(Self::Delete),
            param => Err(C8yJsonOverMqttDeserializerError::InvalidParameter {
                parameter: param.into(),
                operation: "c8y_SoftwareUpdate".into(),
                hint: "It must be install or delete.".into(),
            }),
        }
    }
}

impl C8ySoftwareUpdate {
    pub fn modules(&self) -> &Vec<C8ySoftwareUpdateModule> {
        &self.lists
    }

    pub fn into_software_update_command(
        &self,
        target: &EntityTopicId,
        cmd_id: String,
    ) -> Result<SoftwareUpdateCommand, C8yJsonOverMqttDeserializerError> {
        let mut request = SoftwareUpdateCommand::new(target, cmd_id);
        for module in self.modules() {
            match module.action.clone().try_into()? {
                C8ySoftwareUpdateAction::Install => {
                    request.add_update(SoftwareModuleUpdate::Install {
                        module: SoftwareModule {
                            module_type: module.get_module_version_and_type().1,
                            name: module.name.clone(),
                            version: module.get_module_version_and_type().0,
                            url: module.get_url(),
                            file_path: None,
                        },
                    });
                }
                C8ySoftwareUpdateAction::Delete => {
                    request.add_update(SoftwareModuleUpdate::Remove {
                        module: SoftwareModule {
                            module_type: module.get_module_version_and_type().1,
                            name: module.name.clone(),
                            version: module.get_module_version_and_type().0,
                            url: None,
                            file_path: None,
                        },
                    });
                }
            }
        }
        Ok(request)
    }
}

impl C8ySoftwareUpdateModule {
    fn get_module_version_and_type(&self) -> (Option<String>, Option<String>) {
        if self.version.is_empty() {
            (None, None) // (empty)
        } else {
            let split = if self.version.matches("::").count() > 1 {
                self.version.rsplit_once("::")
            } else {
                self.version.split_once("::")
            };

            match split {
                Some((v, t)) => {
                    if v.is_empty() {
                        (None, Some(t.into())) // ::debian
                    } else if !t.is_empty() {
                        (Some(v.into()), Some(t.into())) // 1.0::debian
                    } else {
                        (Some(v.into()), None)
                    }
                }
                None => {
                    if self.version == " " {
                        (None, None) // as long as c8y UI forces version input
                    } else {
                        (Some(self.version.clone()), None) // 1.0
                    }
                }
            }
        }
    }

    fn get_url(&self) -> Option<DownloadInfo> {
        match &self.url {
            Some(url) if url.trim().is_empty() => None,
            Some(url) => Some(DownloadInfo::new(url.as_str())),
            None => None,
        }
    }
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

/// Representation of c8y_Firmware JSON object
///
/// ```rust
/// use c8y_api::json_c8y_deserializer::C8yFirmware;
///
/// // Example input from c8y
/// let data = r#"
/// {
///     "name": "foo",
///     "version": "1.0.2",
///     "url": "https://dummy.url/firmware.zip"
/// }"#;
///
/// // Parse the data
/// let req: C8yFirmware = serde_json::from_str(data).unwrap();
/// ```
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8yFirmware {
    pub name: String,
    pub version: String,
    pub url: String,
}

pub trait C8yJsonOverMqttOperation {
    fn from_value(value: serde_json::Value) -> Result<Self, serde_json::Error>
    where
        Self: Sized + serde::de::DeserializeOwned,
    {
        serde_json::from_value(value.clone())
    }
}

impl C8yJsonOverMqttOperation for C8yRestart {}
impl C8yJsonOverMqttOperation for C8ySoftwareUpdate {}
impl C8yJsonOverMqttOperation for C8yLogfileRequest {}
impl C8yJsonOverMqttOperation for C8yUploadConfigFile {}
impl C8yJsonOverMqttOperation for C8yDownloadConfigFile {}
impl C8yJsonOverMqttOperation for C8yFirmware {}

#[derive(thiserror::Error, Debug)]
pub enum C8yJsonOverMqttDeserializerError {
    #[error("Parameter {parameter} is not recognized. {hint}")]
    InvalidParameter {
        operation: String,
        parameter: String,
        hint: String,
    },
}
