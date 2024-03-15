use crate::smartrest::smartrest_deserializer::to_datetime;
use download::DownloadInfo;
use mqtt_channel::Topic;
use serde::Deserialize;
use std::collections::HashMap;
use tedge_api::mqtt_topics::EntityTopicId;
use tedge_api::SoftwareModule;
use tedge_api::SoftwareModuleUpdate;
use tedge_api::SoftwareUpdateCommand;
use tedge_config::TopicPrefix;
use time::OffsetDateTime;

pub struct C8yDeviceControlTopic;

impl C8yDeviceControlTopic {
    pub fn topic(prefix: &TopicPrefix) -> Topic {
        Topic::new_unchecked(&Self::name(prefix))
    }

    pub fn accept(topic: &Topic, prefix: &TopicPrefix) -> bool {
        topic.name.starts_with(&Self::name(prefix))
    }

    pub fn name(prefix: &TopicPrefix) -> String {
        format!("{prefix}/devicecontrol/notifications")
    }
}

#[derive(Debug)]
pub enum C8yDeviceControlOperation {
    Restart(C8yRestart),
    SoftwareUpdate(C8ySoftwareUpdate),
    LogfileRequest(C8yLogfileRequest),
    UploadConfigFile(C8yUploadConfigFile),
    DownloadConfigFile(C8yDownloadConfigFile),
    Firmware(C8yFirmware),
    Custom,
}

impl C8yDeviceControlOperation {
    pub fn from_json_object(
        hashmap: &HashMap<String, serde_json::Value>,
    ) -> Result<Self, serde_json::Error> {
        let op = if let Some(value) = hashmap.get("c8y_Restart") {
            C8yDeviceControlOperation::Restart(C8yRestart::from_json_value(value.clone())?)
        } else if let Some(value) = hashmap.get("c8y_SoftwareUpdate") {
            C8yDeviceControlOperation::SoftwareUpdate(C8ySoftwareUpdate::from_json_value(
                value.clone(),
            )?)
        } else if let Some(value) = hashmap.get("c8y_LogfileRequest") {
            C8yDeviceControlOperation::LogfileRequest(C8yLogfileRequest::from_json_value(
                value.clone(),
            )?)
        } else if let Some(value) = hashmap.get("c8y_UploadConfigFile") {
            C8yDeviceControlOperation::UploadConfigFile(C8yUploadConfigFile::from_json_value(
                value.clone(),
            )?)
        } else if let Some(value) = hashmap.get("c8y_DownloadConfigFile") {
            C8yDeviceControlOperation::DownloadConfigFile(C8yDownloadConfigFile::from_json_value(
                value.clone(),
            )?)
        } else if let Some(value) = hashmap.get("c8y_Firmware") {
            C8yDeviceControlOperation::Firmware(C8yFirmware::from_json_value(value.clone())?)
        } else {
            C8yDeviceControlOperation::Custom
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
    // None if c8y's version is older than 10.14. See issue #1352
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
        let trimmed_version = self.version.trim();

        // For C8Y version >= 10.4, refer to the issue #1352
        match &self.software_type {
            Some(module_type) if !module_type.trim().is_empty() => {
                let version = (!trimmed_version.is_empty()).then(|| trimmed_version.to_string());
                return (version, Some(module_type.into()));
            }
            _ => {}
        }

        // For C8Y version < 10.4, version field is supposed to be <version>::<sw_type>
        if trimmed_version.is_empty() {
            (None, None) // (empty)
        } else {
            match trimmed_version.rsplit_once("::") {
                None => (Some(trimmed_version.into()), None), // 1.0
                Some((v, t)) => {
                    match (v.is_empty(), t.is_empty()) {
                        (true, true) => (None, None),                       // ::
                        (true, false) => (None, Some(t.into())),            // ::debian
                        (false, true) => (Some(v.into()), None),            // 1.0::
                        (false, false) => (Some(v.into()), Some(t.into())), // 1.0::debian
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

pub trait C8yDeviceControlOperationHelper {
    fn from_json_value(value: serde_json::Value) -> Result<Self, serde_json::Error>
    where
        Self: Sized + serde::de::DeserializeOwned,
    {
        serde_json::from_value(value.clone())
    }
}

impl C8yDeviceControlOperationHelper for C8yRestart {}

impl C8yDeviceControlOperationHelper for C8ySoftwareUpdate {}

impl C8yDeviceControlOperationHelper for C8yLogfileRequest {}

impl C8yDeviceControlOperationHelper for C8yUploadConfigFile {}

impl C8yDeviceControlOperationHelper for C8yDownloadConfigFile {}

impl C8yDeviceControlOperationHelper for C8yFirmware {}

#[derive(thiserror::Error, Debug)]
pub enum C8yJsonOverMqttDeserializerError {
    #[error("Parameter {parameter} is not recognized. {hint}")]
    InvalidParameter {
        operation: String,
        parameter: String,
        hint: String,
    },
}

#[cfg(test)]
mod tests {
    use crate::json_c8y_deserializer::C8yDeviceControlOperationHelper;
    use crate::json_c8y_deserializer::C8yOperation;
    use crate::json_c8y_deserializer::C8ySoftwareUpdate;
    use crate::json_c8y_deserializer::C8ySoftwareUpdateModule;
    use assert_json_diff::assert_json_eq;
    use serde_json::json;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::Jsonify;
    use tedge_api::SoftwareModule;
    use tedge_api::SoftwareModuleUpdate;
    use tedge_api::SoftwareUpdateCommand;

    #[test]
    fn verify_get_module_version_and_type() {
        let mut module = C8ySoftwareUpdateModule {
            name: "software1".into(),
            version: "".into(),
            url: None,
            software_type: None,
            action: "install".into(),
            id: None,
        }; // ""
        assert_eq!(module.get_module_version_and_type(), (None, None));

        module.version = " ".into(); // " " (space)
        assert_eq!(module.get_module_version_and_type(), (None, None));

        module.version = "   ".into(); // "   " (more spaces)
        assert_eq!(module.get_module_version_and_type(), (None, None));

        module.version = "::".into();
        assert_eq!(module.get_module_version_and_type(), (None, None));

        module.version = "::debian".into();
        assert_eq!(
            module.get_module_version_and_type(),
            (None, Some("debian".to_string()))
        );

        module.version = "1.0.0::debian".into();
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0".to_string()), Some("debian".to_string()))
        );

        module.version = "1.0.0::1::debian".into();
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0::1".to_string()), Some("debian".to_string()))
        );

        module.version = "1.0.0::1::".into();
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0::1".to_string()), None)
        );

        module.version = "1.0.0".into();
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0".to_string()), None)
        );
    }

    #[test]
    fn verify_get_module_version_and_type_with_type_support_1352() {
        let mut module = C8ySoftwareUpdateModule {
            name: "software1".into(),
            version: "".into(),
            url: None,
            software_type: Some("rpm".into()),
            action: "install".into(),
            id: None,
        }; // ""
        assert_eq!(
            module.get_module_version_and_type(),
            (None, Some("rpm".to_string()))
        );

        module.version = " ".into(); // " " (space)
        assert_eq!(
            module.get_module_version_and_type(),
            (None, Some("rpm".to_string()))
        );

        module.version = "   ".into(); // "   " (more spaces)
        assert_eq!(
            module.get_module_version_and_type(),
            (None, Some("rpm".to_string()))
        );

        module.version = "1.0.0".into();
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0".to_string()), Some("rpm".to_string()))
        );

        // If software_type has valid value, don't use "::" logic.
        module.version = "1.0.0::debian".into();
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0::debian".to_string()), Some("rpm".to_string()))
        );

        // If software_type has invalid value, fall back to "::" logic.
        module.software_type = Some("  ".into());
        module.version = "1.0.0::debian".into();
        assert_eq!(
            module.get_module_version_and_type(),
            (Some("1.0.0".to_string()), Some("debian".to_string()))
        );
    }

    #[test]
    fn deserialize_incorrect_software_update_action() {
        let device = EntityTopicId::default_main_device();

        let data = json!([
            {
                "name": "bar",
                "action": "unknown",
                "version": "1.0.1"
            }
        ]);

        assert!(serde_json::from_str::<C8ySoftwareUpdate>(&data.to_string())
            .unwrap()
            .into_software_update_command(&device, "123".to_string())
            .is_err());
    }

    #[test]
    fn from_json_over_mqtt_update_software_to_software_update_cmd() {
        let json_over_mqtt_payload = json!(
        {
            "delivery": {
                "log": [],
                "time": "2023-02-08T06:51:19.350Z",
                "status": "PENDING"
            },
            "agentId": "22519994",
            "creationTime": "2023-02-08T06:51:19.318Z",
            "deviceId": "22519994",
            "id": "522559",
            "status": "PENDING",
            "description": "test operation",
            "c8y_SoftwareUpdate": [
                {
                    "name": "software1",
                    "action": "install",
                    "id": "123456",
                    "version": "version1::debian",
                    "url": "url1"
                },
                {
                    "softwareType": "rpm",
                    "name": "software2",
                    "action": "delete",
                    "version": "1.0"
                },
                {
                    "softwareType": "",
                    "name": "software3",
                    "action": "delete",
                    "version": ""
                }
            ],
            "externalSource": {
                "externalId": "external_id",
                "type": "c8y_Serial"
            }
        });

        let op: C8yOperation = serde_json::from_str(&json_over_mqtt_payload.to_string()).unwrap();
        let req = C8ySoftwareUpdate::from_json_value(
            op.extras
                .get("c8y_SoftwareUpdate")
                .expect("c8y_SoftwareUpdate field is missing")
                .to_owned(),
        )
        .expect("Failed to deserialize");
        let device = EntityTopicId::default_main_device();
        let thin_edge_json = req
            .into_software_update_command(&device, "123".to_string())
            .unwrap();

        let mut expected_thin_edge_json = SoftwareUpdateCommand::new(&device, "123".to_string());
        expected_thin_edge_json.add_update(SoftwareModuleUpdate::install(SoftwareModule {
            module_type: Some("debian".to_string()),
            name: "software1".to_string(),
            version: Some("version1".to_string()),
            url: Some("url1".into()),
            file_path: None,
        }));
        expected_thin_edge_json.add_update(SoftwareModuleUpdate::remove(SoftwareModule {
            module_type: Some("rpm".to_string()),
            name: "software2".to_string(),
            version: Some("1.0".to_string()),
            url: None,
            file_path: None,
        }));
        expected_thin_edge_json.add_update(SoftwareModuleUpdate::remove(SoftwareModule {
            module_type: None,
            name: "software3".to_string(),
            version: None,
            url: None,
            file_path: None,
        }));

        assert_eq!(thin_edge_json, expected_thin_edge_json);
    }

    #[test]
    fn from_c8y_json_to_thin_edge_software_update_json() {
        let data = json!([
            {
                "name": "nodered",
                "action": "install",
                "version": "1.0.0::debian",
                "url": ""
            },
            {
                "name": "collectd",
                "action": "install",
                "version": "5.7::debian",
                "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2"
            },
            {
                "softwareType": "debian",
                "name": "nano",
                "action": "delete",
                "version": "2.3"
            },
            {
                "name": "nginx",
                "action": "install",
                "version": "1.21.0::docker",
                "url": ""
            },
            {
                "name": "mongodb",
                "action": "delete",
                "version": "4.4.6::docker"
            }
        ]);

        let req: C8ySoftwareUpdate = serde_json::from_str(&data.to_string()).unwrap();

        let software_update_request = req
            .into_software_update_command(&EntityTopicId::default_main_device(), "123".to_string())
            .unwrap();

        let output_json = software_update_request.payload.to_json();

        let expected_json = json!({
            "status": "init",
            "updateList": [
                {
                    "type": "debian",
                    "modules": [
                        {
                            "name": "nodered",
                            "version": "1.0.0",
                            "action": "install"
                        },
                        {
                            "name": "collectd",
                            "version": "5.7",
                            "url": "https://collectd.org/download/collectd-tarballs/collectd-5.12.0.tar.bz2",
                            "action": "install"
                        },
                        {
                            "name": "nano",
                            "version": "2.3",
                            "action": "remove"
                        }
                    ]
                },
                {
                    "type": "docker",
                    "modules": [
                        {
                            "name": "nginx",
                            "version": "1.21.0",
                            "action": "install"
                        },
                        {
                            "name": "mongodb",
                            "version": "4.4.6",
                            "action": "remove"
                        }
                    ]
                }
            ]
        });
        assert_json_eq!(
            serde_json::from_str::<serde_json::Value>(output_json.as_str()).unwrap(),
            expected_json
        );
    }

    #[test]
    fn access_c8y_software_update_modules() {
        let data = json!([
            {
                "softwareType": "debian",
                "name": "software1",
                "action": "install",
                "version": "version1",
                "url": "url1"
            },
            {
                "name": "software2",
                "action": "delete",
                "version": ""
            }
        ]);

        let update_software = serde_json::from_str::<C8ySoftwareUpdate>(&data.to_string()).unwrap();

        let expected_vec = vec![
            C8ySoftwareUpdateModule {
                name: "software1".into(),
                version: "version1".into(),
                url: Some("url1".into()),
                software_type: Some("debian".into()),
                action: "install".into(),
                id: None,
            },
            C8ySoftwareUpdateModule {
                name: "software2".into(),
                version: "".into(),
                url: None,
                software_type: None,
                action: "delete".into(),
                id: None,
            },
        ];

        assert_eq!(update_software.modules(), &expected_vec);
    }
}
