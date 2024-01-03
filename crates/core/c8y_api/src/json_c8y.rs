use download::DownloadInfo;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use tedge_api::alarm::ThinEdgeAlarm;
use tedge_api::alarm::ThinEdgeAlarmData;
use tedge_api::entity_store::EntityMetadata;
use tedge_api::entity_store::EntityType;
use tedge_api::event::ThinEdgeEvent;
use tedge_api::messages::SoftwareListCommand;
use tedge_api::EntityStore;
use tedge_api::Jsonify;
use tedge_api::SoftwareModule;
use tedge_api::SoftwareType;
use tedge_api::SoftwareVersion;
use time::OffsetDateTime;

const EMPTY_STRING: &str = "";
const DEFAULT_ALARM_SEVERITY: AlarmSeverity = AlarmSeverity::Minor;
const DEFAULT_ALARM_TYPE: &str = "ThinEdgeAlarm";

#[derive(Debug, Serialize, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub struct C8yCreateEvent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<C8yManagedObject>,

    #[serde(rename = "type")]
    pub event_type: String,

    #[serde(with = "time::serde::rfc3339")]
    pub time: OffsetDateTime,

    pub text: String,

    #[serde(flatten)]
    pub extras: HashMap<String, Value>,
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
/// used to retrieve the id of a log event
pub struct C8yEventResponse {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct C8yManagedObject {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InternalIdResponse {
    managed_object: C8yManagedObject,
    external_id: String,
}

impl InternalIdResponse {
    pub fn new(id: &str, external_id: &str) -> Self {
        InternalIdResponse {
            managed_object: C8yManagedObject { id: id.to_string() },
            external_id: external_id.to_string(),
        }
    }

    pub fn id(&self) -> String {
        self.managed_object.id.clone()
    }
}

#[derive(Debug, Deserialize, Serialize, Eq, PartialEq)]
pub struct C8ySoftwareModuleItem {
    pub name: String,
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(flatten)]
    pub url: Option<DownloadInfo>,
}

impl<'a> Jsonify<'a> for C8ySoftwareModuleItem {}

impl From<SoftwareModule> for C8ySoftwareModuleItem {
    fn from(module: SoftwareModule) -> Self {
        let url = if module.url.is_none() {
            Some(EMPTY_STRING.into())
        } else {
            module.url
        };

        Self {
            name: module.name,
            version: Option::from(combine_version_and_type(
                &module.version,
                &module.module_type,
            )),
            url,
        }
    }
}

#[derive(Debug, Serialize, Eq, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct C8yUpdateSoftwareListResponse {
    #[serde(rename = "c8y_SoftwareList")]
    c8y_software_list: Option<Vec<C8ySoftwareModuleItem>>,
}

impl<'a> Jsonify<'a> for C8yUpdateSoftwareListResponse {}

impl From<&SoftwareListCommand> for C8yUpdateSoftwareListResponse {
    fn from(list: &SoftwareListCommand) -> Self {
        let mut new_list: Vec<C8ySoftwareModuleItem> = Vec::new();
        list.modules().into_iter().for_each(|software_module| {
            let c8y_software_module: C8ySoftwareModuleItem = software_module.into();
            new_list.push(c8y_software_module);
        });

        Self {
            c8y_software_list: Some(new_list),
        }
    }
}

impl From<ThinEdgeEvent> for C8yCreateEvent {
    fn from(event: ThinEdgeEvent) -> Self {
        let mut extras = HashMap::new();
        if let Some(source) = event.source {
            update_the_external_source_event(&mut extras, &source);
        }

        match event.data {
            None => Self {
                source: None,
                event_type: event.name.clone(),
                time: OffsetDateTime::now_utc(),
                text: event.name,
                extras,
            },
            Some(event_data) => {
                extras.extend(event_data.extras);

                // If payload contains type, use the value as the event type unless it's empty
                let event_type = match extras.remove("type") {
                    Some(type_from_payload) => match type_from_payload.as_str() {
                        Some(new_type) if !new_type.is_empty() => new_type.to_string(),
                        _ => event.name,
                    },
                    None => event.name,
                };

                Self {
                    source: None,
                    event_type: event_type.clone(),
                    time: event_data.time.unwrap_or_else(OffsetDateTime::now_utc),
                    text: event_data.text.unwrap_or(event_type),
                    extras,
                }
            }
        }
    }
}

impl<'a> Jsonify<'a> for C8yCreateEvent {}

fn combine_version_and_type(
    version: &Option<SoftwareVersion>,
    module_type: &Option<SoftwareType>,
) -> String {
    match module_type {
        Some(m) => {
            if m.is_empty() {
                match version {
                    Some(v) => v.into(),
                    None => EMPTY_STRING.into(),
                }
            } else {
                match version {
                    Some(v) => format!("{}::{}", v, m),
                    None => format!("::{}", m),
                }
            }
        }
        None => match version {
            Some(v) => {
                if v.contains("::") {
                    format!("{}::", v)
                } else {
                    v.into()
                }
            }
            None => EMPTY_STRING.into(),
        },
    }
}

fn update_the_external_source_event(extras: &mut HashMap<String, Value>, source: &str) {
    let mut value = serde_json::Map::new();
    value.insert("externalId".to_string(), source.into());
    value.insert("type".to_string(), "c8y_Serial".into());
    extras.insert("externalSource".into(), value.into());
}

fn make_c8y_source_fragment(source_name: &str) -> SourceInfo {
    SourceInfo::new(source_name.into(), "c8y_Serial".into())
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    #[serde(rename = "externalId")]
    pub id: String,
    #[serde(rename = "type")]
    pub source_type: String,
}

impl SourceInfo {
    pub fn new(id: String, source_type: String) -> Self {
        Self { id, source_type }
    }
}

/// Internal representation of c8y's alarm model.
#[derive(Debug, PartialEq, Eq)]
pub enum C8yAlarm {
    Create(C8yCreateAlarm),
    Clear(C8yClearAlarm),
}

/// Internal representation of creating an alarm in c8y.
/// Note: text and time are optional for SmartREST, however,
/// mandatory for JSON over MQTT. Hence, here they are mandatory.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct C8yCreateAlarm {
    /// Alarm type, default is "ThinEdgeAlarm".
    #[serde(rename = "type")]
    pub alarm_type: String,

    /// None for main device, Some for child device.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "externalSource")]
    pub source: Option<SourceInfo>,

    pub severity: AlarmSeverity,

    pub text: String,

    #[serde(with = "time::serde::rfc3339")]
    pub time: OffsetDateTime,

    #[serde(flatten)]
    pub fragments: HashMap<String, Value>,
}

/// Internal representation of clearing an alarm in c8y.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct C8yClearAlarm {
    /// Alarm type, default is "ThinEdgeAlarm".
    #[serde(rename = "type")]
    pub alarm_type: String,

    /// None for main device, Some for child device.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "externalSource")]
    pub source: Option<SourceInfo>,
}

impl C8yAlarm {
    pub fn try_from(
        alarm: &ThinEdgeAlarm,
        entity_store: &EntityStore,
    ) -> Result<Self, C8yAlarmError> {
        if let Some(entity) = entity_store.get(&alarm.source) {
            let source = Self::convert_source(entity);
            let alarm_type = Self::convert_alarm_type(&alarm.alarm_type);

            let c8y_alarm = match alarm.data.as_ref() {
                None => C8yAlarm::Clear(C8yClearAlarm { alarm_type, source }),
                Some(tedge_alarm_data) => C8yAlarm::Create(C8yCreateAlarm {
                    alarm_type: alarm_type.clone(),
                    source,
                    severity: C8yCreateAlarm::convert_severity(tedge_alarm_data),
                    text: C8yCreateAlarm::convert_text(tedge_alarm_data, &alarm_type),
                    time: C8yCreateAlarm::convert_time(tedge_alarm_data),
                    fragments: C8yCreateAlarm::convert_extras(tedge_alarm_data),
                }),
            };
            Ok(c8y_alarm)
        } else {
            Err(C8yAlarmError::UnsupportedDeviceTopicId(
                alarm.source.to_string(),
            ))
        }
    }

    fn convert_source(entity: &EntityMetadata) -> Option<SourceInfo> {
        match entity.r#type {
            EntityType::MainDevice => None,
            EntityType::ChildDevice => Some(make_c8y_source_fragment(entity.external_id.as_ref())),
            EntityType::Service => Some(make_c8y_source_fragment(entity.external_id.as_ref())),
        }
    }

    fn convert_alarm_type(alarm_type: &str) -> String {
        if alarm_type.is_empty() {
            DEFAULT_ALARM_TYPE.to_string()
        } else {
            alarm_type.to_string()
        }
    }
}

impl C8yCreateAlarm {
    fn convert_severity(alarm_data: &ThinEdgeAlarmData) -> AlarmSeverity {
        match alarm_data.severity.clone() {
            Some(severity) => match AlarmSeverity::try_from(severity.as_str()) {
                Ok(c8y_severity) => c8y_severity,
                Err(_) => DEFAULT_ALARM_SEVERITY,
            },
            None => DEFAULT_ALARM_SEVERITY,
        }
    }

    fn convert_text(alarm_data: &ThinEdgeAlarmData, alarm_type: &str) -> String {
        alarm_data.text.clone().unwrap_or(alarm_type.to_string())
    }

    fn convert_time(alarm_data: &ThinEdgeAlarmData) -> OffsetDateTime {
        alarm_data.time.unwrap_or_else(OffsetDateTime::now_utc)
    }

    /// Remove reserved keywords from extras.
    /// "type", "time", "text", "severity" are ensured that they are not
    /// in the hashmap of ThinEdgeAlarm because they are already members of the struct itself.
    fn convert_extras(alarm_data: &ThinEdgeAlarmData) -> HashMap<String, Value> {
        let mut map = alarm_data.extras.clone();
        map.remove("externalSource");
        map
    }
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all(serialize = "UPPERCASE"))]
pub enum AlarmSeverity {
    Critical,
    Major,
    Minor,
    Warning,
}

impl TryFrom<&str> for AlarmSeverity {
    type Error = C8yAlarmError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "critical" => Ok(AlarmSeverity::Critical),
            "major" => Ok(AlarmSeverity::Major),
            "minor" => Ok(AlarmSeverity::Minor),
            "warning" => Ok(AlarmSeverity::Warning),
            invalid => Err(C8yAlarmError::UnsupportedAlarmSeverity(invalid.into())),
        }
    }
}

impl fmt::Display for AlarmSeverity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AlarmSeverity::Critical => write!(f, "CRITICAL"),
            AlarmSeverity::Major => write!(f, "MAJOR"),
            AlarmSeverity::Minor => write!(f, "MINOR"),
            AlarmSeverity::Warning => write!(f, "WARNING"),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum C8yAlarmError {
    #[error("Unsupported alarm severity in topic: {0}")]
    UnsupportedAlarmSeverity(String),

    #[error("Unsupported device topic ID in topic: {0}")]
    UnsupportedDeviceTopicId(String),
}

#[cfg(test)]
mod tests {
    use crate::json_c8y::AlarmSeverity;
    use anyhow::Result;
    use assert_matches::assert_matches;
    use mqtt_channel::Message;
    use mqtt_channel::Topic;
    use serde_json::json;
    use std::collections::HashSet;
    use tedge_api::alarm::ThinEdgeAlarm;
    use tedge_api::alarm::ThinEdgeAlarmData;
    use tedge_api::entity_store::EntityExternalId;
    use tedge_api::entity_store::EntityRegistrationMessage;
    use tedge_api::entity_store::InvalidExternalIdError;
    use tedge_api::event::ThinEdgeEventData;
    use tedge_api::messages::SoftwareListCommandPayload;
    use tedge_api::mqtt_topics::EntityTopicId;
    use tedge_api::mqtt_topics::MqttSchema;
    use test_case::test_case;
    use time::macros::datetime;

    use super::*;

    #[test]
    fn from_software_module_to_c8y_software_module_item() {
        let software_module = SoftwareModule {
            module_type: Some("a".into()),
            name: "b".into(),
            version: Some("c".into()),
            url: Some("".into()),
            file_path: None,
        };

        let expected_c8y_item = C8ySoftwareModuleItem {
            name: "b".into(),
            version: Some("c::a".into()),
            url: Some("".into()),
        };

        let converted: C8ySoftwareModuleItem = software_module.into();

        assert_eq!(converted, expected_c8y_item);
    }

    #[test]
    fn from_thin_edge_json_to_c8y_set_software_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[ 
                {"type":"debian", "modules":[
                    {"name":"a"},
                    {"name":"b","version":"1.0"},
                    {"name":"c","url":"https://foobar.io/c.deb"},
                    {"name":"d","version":"beta","url":"https://foobar.io/d.deb"}
                ]},
                {"type":"apama","modules":[
                    {"name":"m","url":"https://foobar.io/m.epl"}
                ]}
            ]}"#;

        let command = SoftwareListCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1".to_string(),
            payload: SoftwareListCommandPayload::from_json(input_json).unwrap(),
        };

        let c8y_software_list: C8yUpdateSoftwareListResponse = (&command).into();

        let expected_struct = C8yUpdateSoftwareListResponse {
            c8y_software_list: Some(vec![
                C8ySoftwareModuleItem {
                    name: "a".into(),
                    version: Some("::debian".into()),
                    url: Some("".into()),
                },
                C8ySoftwareModuleItem {
                    name: "b".into(),
                    version: Some("1.0::debian".into()),
                    url: Some("".into()),
                },
                C8ySoftwareModuleItem {
                    name: "c".into(),
                    version: Some("::debian".into()),
                    url: Some("https://foobar.io/c.deb".into()),
                },
                C8ySoftwareModuleItem {
                    name: "d".into(),
                    version: Some("beta::debian".into()),
                    url: Some("https://foobar.io/d.deb".into()),
                },
                C8ySoftwareModuleItem {
                    name: "m".into(),
                    version: Some("::apama".into()),
                    url: Some("https://foobar.io/m.epl".into()),
                },
            ]),
        };

        let expected_json = r#"{"c8y_SoftwareList":[{"name":"a","version":"::debian","url":""},{"name":"b","version":"1.0::debian","url":""},{"name":"c","version":"::debian","url":"https://foobar.io/c.deb"},{"name":"d","version":"beta::debian","url":"https://foobar.io/d.deb"},{"name":"m","version":"::apama","url":"https://foobar.io/m.epl"}]}"#;

        assert_eq!(c8y_software_list, expected_struct);
        assert_eq!(c8y_software_list.to_json(), expected_json);
    }

    #[test]
    fn empty_to_c8y_set_software_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[]
            }"#;

        let command = &SoftwareListCommand {
            target: EntityTopicId::default_main_device(),
            cmd_id: "1".to_string(),
            payload: SoftwareListCommandPayload::from_json(input_json).unwrap(),
        };

        let c8y_software_list: C8yUpdateSoftwareListResponse = command.into();

        let expected_struct = C8yUpdateSoftwareListResponse {
            c8y_software_list: Some(vec![]),
        };
        let expected_json = r#"{"c8y_SoftwareList":[]}"#;

        assert_eq!(c8y_software_list, expected_struct);
        assert_eq!(c8y_software_list.to_json(), expected_json);
    }

    #[test]
    fn get_id_from_c8y_response() {
        let managed_object = C8yManagedObject { id: "12345".into() };
        let response = InternalIdResponse {
            managed_object,
            external_id: "test".into(),
        };

        assert_eq!(response.id(), "12345".to_string());
    }

    #[test]
    fn verify_combine_version_and_type() {
        let some_version: Option<SoftwareVersion> = Some("1.0".to_string());
        let some_version_with_colon: Option<SoftwareVersion> = Some("1.0.0::1".to_string());
        let none_version: Option<SoftwareVersion> = None;
        let some_module_type: Option<SoftwareType> = Some("debian".to_string());
        let none_module_type: Option<SoftwareType> = None;

        assert_eq!(
            combine_version_and_type(&some_version, &some_module_type),
            "1.0::debian"
        );
        assert_eq!(
            combine_version_and_type(&some_version, &none_module_type),
            "1.0"
        );
        assert_eq!(
            combine_version_and_type(&some_version_with_colon, &some_module_type),
            "1.0.0::1::debian"
        );
        assert_eq!(
            combine_version_and_type(&some_version_with_colon, &none_module_type),
            "1.0.0::1::"
        );
        assert_eq!(
            combine_version_and_type(&none_version, &some_module_type),
            "::debian"
        );
        assert_eq!(
            combine_version_and_type(&none_version, &none_module_type),
            EMPTY_STRING
        );
    }

    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone clicked".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: None,
        },
        C8yCreateEvent {
            source: None,
            event_type: "click_event".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "Someone clicked".into(),
            extras: HashMap::new(),
        }
        ;"event translation"
    )]
    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: None,
        },
        C8yCreateEvent {
            source: None,
            event_type: "click_event".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "click_event".into(),
            extras: HashMap::new(),
        }
        ;"event translation without text"
    )]
    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone, clicked, it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: None,
        },
        C8yCreateEvent {
            source: None,
            event_type: "click_event".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "Someone, clicked, it".into(),
            extras: HashMap::new(),
        }
        ;"event translation with commas in text"
    )]
    fn check_event_translation(
        tedge_event: ThinEdgeEvent,
        expected_c8y_event: C8yCreateEvent,
    ) -> Result<()> {
        let actual_c8y_event = C8yCreateEvent::from(tedge_event);

        assert_eq!(expected_c8y_event, actual_c8y_event);

        Ok(())
    }

    #[test]
    fn event_translation_empty_json_payload_generates_timestamp() -> Result<()> {
        let tedge_event = ThinEdgeEvent {
            name: "empty_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
            source: None,
        };

        let actual_c8y_event = C8yCreateEvent::from(tedge_event);

        assert_eq!(actual_c8y_event.event_type, "empty_event".to_string());
        assert_eq!(actual_c8y_event.text, "empty_event".to_string());
        assert_matches!(actual_c8y_event.time, _);
        assert_matches!(actual_c8y_event.source, None);
        assert!(actual_c8y_event.extras.is_empty());

        Ok(())
    }

    #[test]
    fn event_translation_empty_payload() -> Result<()> {
        let tedge_event = ThinEdgeEvent {
            name: "empty_event".into(),
            data: None,
            source: None,
        };

        let actual_c8y_event = C8yCreateEvent::from(tedge_event);

        assert_eq!(actual_c8y_event.event_type, "empty_event".to_string());
        assert_eq!(actual_c8y_event.text, "empty_event".to_string());
        assert!(actual_c8y_event.time < OffsetDateTime::now_utc());
        assert_matches!(actual_c8y_event.source, None);
        assert!(actual_c8y_event.extras.is_empty());

        Ok(())
    }

    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "temperature alarm".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
        },
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature alarm".into(),
            source: None,
            severity: AlarmSeverity::Critical,
            text: "Temperature went high".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: HashMap::new(),
        })
        ;"critical alarm translation"
    )]
    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "temperature alarm".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
            }),
        },
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature alarm".into(),
            source: None,
            severity: AlarmSeverity::Critical,
            text: "Temperature went high".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
        })
        ;"critical alarm translation with custom fragment"
    )]
    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "temperature alarm".into(),
            source: EntityTopicId::default_child_device("external_source").unwrap(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
            }),
        },
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature alarm".into(),
            source: Some(SourceInfo::new("external_source".to_string(),"c8y_Serial".to_string())),
            severity: AlarmSeverity::Critical,
            text: "Temperature went high".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
        })
        ;"critical alarm translation of child device with custom fragment"
    )]
    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("invalid".into()),
                text: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
        },
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "ThinEdgeAlarm".into(),
            source: None,
            severity: AlarmSeverity::Minor,
            text: "ThinEdgeAlarm".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: HashMap::new(),
        })
        ;"using default values of alarm"
    )]
    #[test_case(
        ThinEdgeAlarm {
            alarm_type: "".into(),
            source: EntityTopicId::default_main_device(),
            data: None,
        },
        C8yAlarm::Clear(C8yClearAlarm {
            alarm_type: "ThinEdgeAlarm".into(),
            source: None,
        })
        ;"convert to clear alarm"
    )]
    fn check_alarm_translation(tedge_alarm: ThinEdgeAlarm, expected_c8y_alarm: C8yAlarm) {
        let temp_dir = tempfile::tempdir().unwrap();
        let main_device = EntityRegistrationMessage::main_device("test-main".into());
        let mut entity_store = EntityStore::with_main_device_and_default_service_type(
            MqttSchema::default(),
            main_device,
            "service".into(),
            dummy_external_id_mapper,
            dummy_external_id_validator,
            5,
            &temp_dir,
        )
        .unwrap();

        let child_registration = EntityRegistrationMessage::new(&Message::new(
            &Topic::new_unchecked("te/device/external_source//"),
            r#"{"@id": "external_source", "@type": "child-device"}"#,
        ))
        .unwrap();
        entity_store.update(child_registration).unwrap();

        let actual_c8y_alarm = C8yAlarm::try_from(&tedge_alarm, &entity_store).unwrap();
        assert_eq!(actual_c8y_alarm, expected_c8y_alarm);
    }

    #[test]
    fn alarm_translation_generates_timestamp_if_not_given() {
        let tedge_alarm = ThinEdgeAlarm {
            alarm_type: "".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
        };

        let temp_dir = tempfile::tempdir().unwrap();
        let main_device = EntityRegistrationMessage::main_device("test-main".into());
        let entity_store = EntityStore::with_main_device_and_default_service_type(
            MqttSchema::default(),
            main_device,
            "service".into(),
            dummy_external_id_mapper,
            dummy_external_id_validator,
            5,
            &temp_dir,
        )
        .unwrap();

        match C8yAlarm::try_from(&tedge_alarm, &entity_store).unwrap() {
            C8yAlarm::Create(value) => {
                assert!(value.time.millisecond() > 0);
            }
            C8yAlarm::Clear(_) => panic!("Must be C8yAlarm::Create"),
        };
    }

    fn dummy_external_id_mapper(
        entity_topic_id: &EntityTopicId,
        _main_device_xid: &EntityExternalId,
    ) -> EntityExternalId {
        entity_topic_id
            .to_string()
            .trim_end_matches('/')
            .replace('/', ":")
            .into()
    }

    fn dummy_external_id_validator(id: &str) -> Result<EntityExternalId, InvalidExternalIdError> {
        let forbidden_chars = HashSet::from(['/', '+', '#']);
        for c in id.chars() {
            if forbidden_chars.contains(&c) {
                return Err(InvalidExternalIdError {
                    external_id: id.into(),
                    invalid_char: c,
                });
            }
        }
        Ok(id.into())
    }
}
