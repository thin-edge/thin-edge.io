use std::collections::HashMap;

use agent_interface::{
    Jsonify, SoftwareListResponse, SoftwareModule, SoftwareType, SoftwareVersion,
};

use c8y_smartrest::error::SMCumulocityMapperError;
use download::DownloadInfo;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thin_edge_json::event::ThinEdgeEvent;
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

#[derive(Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Debug, Deserialize, Serialize, PartialEq)]
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
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
        let extras;
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use assert_matches::assert_matches;
    use test_case::test_case;
    use thin_edge_json::event::ThinEdgeEventData;
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
}
