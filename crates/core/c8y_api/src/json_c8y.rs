use std::collections::HashMap;

use tedge_api::alarm::ThinEdgeAlarm;
use tedge_api::Jsonify;
use tedge_api::SoftwareListResponse;
use tedge_api::SoftwareModule;
use tedge_api::SoftwareType;
use tedge_api::SoftwareVersion;

use crate::smartrest::error::SMCumulocityMapperError;
use download::DownloadInfo;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use tedge_api::event::ThinEdgeEvent;
use time::OffsetDateTime;

const EMPTY_STRING: &str = "";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct C8yUpdateSoftwareListResponse {
    #[serde(rename = "c8y_SoftwareList")]
    c8y_software_list: Option<Vec<C8ySoftwareModuleItem>>,
}

impl<'a> Jsonify<'a> for C8yUpdateSoftwareListResponse {}

impl From<&SoftwareListResponse> for C8yUpdateSoftwareListResponse {
    fn from(list: &SoftwareListResponse) -> Self {
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

impl C8yCreateEvent {
    pub fn new(
        source: Option<C8yManagedObject>,
        event_type: String,
        time: OffsetDateTime,
        text: String,
        extras: HashMap<String, Value>,
    ) -> Self {
        Self {
            source,
            event_type,
            time,
            text,
            extras,
        }
    }
}

impl TryFrom<ThinEdgeEvent> for C8yCreateEvent {
    type Error = SMCumulocityMapperError;

    fn try_from(event: ThinEdgeEvent) -> Result<Self, SMCumulocityMapperError> {
        let event_type = event.name;
        let text;
        let time;
        let mut extras;
        match event.data {
            None => {
                text = event_type.clone();
                time = OffsetDateTime::now_utc();
                extras = HashMap::new();
            }
            Some(event_data) => {
                text = event_data.text.unwrap_or_else(|| event_type.clone());
                time = event_data.time.unwrap_or_else(OffsetDateTime::now_utc);
                extras = event_data.extras;
            }
        }
        if let Some(source) = event.source {
            update_the_external_source_event(&mut extras, &source)?;
        }

        Ok(Self {
            source: None,
            event_type,
            time,
            text,
            extras,
        })
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
fn update_the_external_source_event(
    extras: &mut HashMap<String, Value>,
    source: &str,
) -> Result<(), SMCumulocityMapperError> {
    let mut value = serde_json::Map::new();
    value.insert("externalId".to_string(), source.into());
    value.insert("type".to_string(), "c8y_Serial".into());
    extras.insert("externalSource".into(), value.into());

    Ok(())
}

fn make_c8y_source_fragment(source_name: &str) -> Option<SourceInfo> {
    Some(SourceInfo::new(source_name.into(), "c8y_Serial".into()))
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct C8yCreateAlarm {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "externalSource")]
    pub source: Option<SourceInfo>,

    pub severity: String,

    #[serde(rename = "type")]
    pub alarm_type: String,

    #[serde(with = "time::serde::rfc3339")]
    pub time: OffsetDateTime,

    pub text: String,

    #[serde(flatten)]
    pub fragments: HashMap<String, Value>,
}

impl C8yCreateAlarm {
    pub fn new(
        source: Option<SourceInfo>,
        severity: String,
        alarm_type: String,
        time: OffsetDateTime,
        text: String,
        fragments: HashMap<String, Value>,
    ) -> Self {
        Self {
            source,
            severity,
            alarm_type,
            time,
            text,
            fragments,
        }
    }
}

impl TryFrom<&ThinEdgeAlarm> for C8yCreateAlarm {
    type Error = SMCumulocityMapperError;

    fn try_from(alarm: &ThinEdgeAlarm) -> Result<Self, SMCumulocityMapperError> {
        let severity = alarm.severity.to_string();
        let alarm_type = alarm.name.to_owned();
        let text;
        let time;
        let fragments;

        match &alarm.to_owned().data {
            None => {
                text = alarm_type.clone();
                time = OffsetDateTime::now_utc();
                fragments = HashMap::new();
            }
            Some(data) => {
                text = data.text.clone().unwrap_or_else(|| alarm_type.clone());
                time = data.time.unwrap_or_else(OffsetDateTime::now_utc);
                fragments = data.alarm_data.clone();
            }
        }

        let source = if let Some(external_source) = &alarm.source {
            make_c8y_source_fragment(external_source)
        } else {
            None
        };

        Ok(Self {
            source,
            severity,
            alarm_type,
            time,
            text,
            fragments,
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use assert_matches::assert_matches;
    use serde_json::json;
    use tedge_api::alarm::AlarmSeverity;
    use tedge_api::alarm::ThinEdgeAlarm;
    use tedge_api::alarm::ThinEdgeAlarmData;
    use tedge_api::event::ThinEdgeEventData;
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

        let json_obj = &SoftwareListResponse::from_json(input_json).unwrap();

        let c8y_software_list: C8yUpdateSoftwareListResponse = json_obj.into();

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
        assert_eq!(c8y_software_list.to_json().unwrap(), expected_json);
    }

    #[test]
    fn empty_to_c8y_set_software_list() {
        let input_json = r#"{
            "id":"1",
            "status":"successful",
            "currentSoftwareList":[]
            }"#;

        let json_obj = &SoftwareListResponse::from_json(input_json).unwrap();
        let c8y_software_list: C8yUpdateSoftwareListResponse = json_obj.into();

        let expected_struct = C8yUpdateSoftwareListResponse {
            c8y_software_list: Some(vec![]),
        };
        let expected_json = r#"{"c8y_SoftwareList":[]}"#;

        assert_eq!(c8y_software_list, expected_struct);
        assert_eq!(c8y_software_list.to_json().unwrap(), expected_json);
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
        let actual_c8y_event = C8yCreateEvent::try_from(tedge_event)?;

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

        let actual_c8y_event = C8yCreateEvent::try_from(tedge_event)?;

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

        let actual_c8y_event = C8yCreateEvent::try_from(tedge_event)?;

        assert_eq!(actual_c8y_event.event_type, "empty_event".to_string());
        assert_eq!(actual_c8y_event.text, "empty_event".to_string());
        assert!(actual_c8y_event.time < OffsetDateTime::now_utc());
        assert_matches!(actual_c8y_event.source, None);
        assert!(actual_c8y_event.extras.is_empty());

        Ok(())
    }

    #[test_case(
        ThinEdgeAlarm {
            name: "temperature alarm".into(),
            severity: AlarmSeverity::Critical,
            data: Some(ThinEdgeAlarmData {
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                alarm_data: HashMap::new(),
            }),
            source: None,
        },
        C8yCreateAlarm {
            severity: "CRITICAL".to_string(),
            source: None,
            alarm_type: "temperature alarm".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "Temperature went high".into(),
            fragments: HashMap::new(),
        }
        ;"critical alarm translation"
    )]
    #[test_case(
        ThinEdgeAlarm {
            name: "temperature alarm".into(),
            severity: AlarmSeverity::Critical,
            data: Some(ThinEdgeAlarmData {
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                alarm_data: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
            }),
            source: None,
        },
        C8yCreateAlarm {
            severity: "CRITICAL".to_string(),
            source: None,
            alarm_type: "temperature alarm".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "Temperature went high".into(),
            fragments: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
        }
        ;"critical alarm translation with custom fragment"
    )]
    #[test_case(
        ThinEdgeAlarm {
            name: "temperature alarm".into(),
            severity: AlarmSeverity::Critical,
            data: Some(ThinEdgeAlarmData {
                text: Some("Temperature went high".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                alarm_data: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
            }),
            source: Some("external_source".into()),
        },
        C8yCreateAlarm {
            severity: "CRITICAL".to_string(),
            source: Some(SourceInfo::new("external_source".to_string(),"c8y_Serial".to_string())),
            alarm_type: "temperature alarm".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            text: "Temperature went high".into(),
            fragments: maplit::hashmap!{"SomeCustomFragment".to_string() => json!({"nested": {"value":"extra info"}})},
        }
        ;"critical alarm translation of child device with custom fragment"
    )]
    fn check_alarm_translation(
        tedge_alarm: ThinEdgeAlarm,
        expected_c8y_alarm: C8yCreateAlarm,
    ) -> Result<()> {
        let actual_c8y_alarm = C8yCreateAlarm::try_from(&tedge_alarm)?;

        assert_eq!(actual_c8y_alarm, expected_c8y_alarm);

        Ok(())
    }
}
