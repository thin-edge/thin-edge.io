use std::collections::HashMap;

use clock::Timestamp;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

use crate::device_id::get_external_identity_from_topic;

use self::error::ThinEdgeJsonDeserializerError;

/// In-memory representation of ThinEdge JSON event.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ThinEdgeEvent {
    #[serde(rename = "type")]
    pub name: String,
    #[serde(flatten)]
    pub data: Option<ThinEdgeEventData>,
    pub source: Option<String>,
}

/// In-memory representation of ThinEdge JSON event payload
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ThinEdgeEventData {
    pub text: Option<String>,

    #[serde(default)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub time: Option<Timestamp>,

    #[serde(flatten)]
    pub extras: HashMap<String, Value>,
}

pub mod error {

    #[derive(thiserror::Error, Debug)]
    pub enum ThinEdgeJsonDeserializerError {
        #[error("Unsupported topic: {0}")]
        UnsupportedTopic(String),

        #[error("Event name can not be empty")]
        EmptyEventName,

        #[error(transparent)]
        SerdeJsonError(#[from] serde_json::error::Error),

        #[error("Parsing of alarm message received on topic: {topic} failed due to error: {error}. Snipped payload: {payload}")]
        FailedToParseJsonPayload {
            topic: String,
            error: String,
            payload: String,
        },

        #[error("Failed to parse as an UTF-8 string the payload received on topic: {topic}, due to error: {error}.")]
        FailedToParsePayloadToString { topic: String, error: String },

        #[error("Unsupported external device ID in topic: {0}")]
        UnsupportedExternalDeviceId(String),
    }
}

impl ThinEdgeEvent {
    /// parent_device_name is needed to create the child device external id
    pub fn try_from(
        parent_device_name: String,
        mqtt_topic: &str,
        mqtt_payload: &str,
    ) -> Result<Self, ThinEdgeJsonDeserializerError> {
        let topic_split: Vec<&str> = mqtt_topic.split('/').collect();
        validate_event_type(mqtt_topic)?;
        if topic_split.len() == 7 {
            let event_name = if let Some(v) = topic_split.last().cloned() {
                v.to_owned()
            } else {
                return Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(
                    mqtt_topic.into(),
                ));
            };

            let event_data = if mqtt_payload.is_empty() {
                None
            } else {
                Some(serde_json::from_str(mqtt_payload)?)
            };

            let external_source = if topic_split[2].eq("main") {
                parent_device_name
            } else {
                get_external_identity_from_topic(parent_device_name, mqtt_topic.into())
                    .unwrap_or_default()
            };

            Ok(Self {
                name: event_name,
                data: event_data,
                source: Some(external_source),
            })
        } else {
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(
                mqtt_topic.into(),
            ))
        }
    }
}

fn validate_event_type(topic: &str) -> Result<(), ThinEdgeJsonDeserializerError> {
    match topic.split('/').collect::<Vec<_>>()[..] {
        ["te", "device", device_id, _, _, "e", event_type] => {
            if device_id.is_empty() {
                Err(ThinEdgeJsonDeserializerError::UnsupportedExternalDeviceId(
                    device_id.into(),
                ))
            } else if event_type.is_empty() {
                Err(ThinEdgeJsonDeserializerError::EmptyEventName)
            } else {
                Ok(())
            }
        }

        _ => Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(
            topic.into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use assert_matches::assert_matches;
    use serde_json::json;
    use serde_json::Value;
    use test_case::test_case;
    use time::macros::datetime;

    #[test]
    fn event_translation_empty_event_name() {
        let result = ThinEdgeEvent::try_from("test-device".into(), "te/device/main///e/", "{}");

        assert_matches!(result, Err(ThinEdgeJsonDeserializerError::EmptyEventName));
    }

    #[test]
    fn event_translation_more_than_four_topic_levels() {
        let result =
            ThinEdgeEvent::try_from("test-device".into(), "tedge/events/page/click/click", "{}");

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(_))
        );
    }

    #[test]
    fn event_translation_empty_payload() -> Result<()> {
        let event_data = ThinEdgeEventData {
            text: Some("foo".to_string()),
            time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            extras: HashMap::new(),
        };

        let serialized = serde_json::to_string(&event_data).unwrap();
        println!("serialized = {}", serialized);

        Ok(())
    }

    #[test]
    fn test_serialize() -> Result<()> {
        let result =
            ThinEdgeEvent::try_from("test-device".into(), "te/device/main///e/click_event", "")?;
        assert_eq!(result.name, "click_event".to_string());
        assert_matches!(result.data, None);
        Ok(())
    }

    #[test]
    fn event_translation_additional_fields() -> Result<()> {
        let event_json = json!({
            "text": "foo",
            "time": "2021-04-23T19:00:00+05:00",
            "extra": "field",
            "numeric": 32u64,
            "complex": {
                "hello": "world",
                "num": 5u32
            }
        });

        let result = ThinEdgeEvent::try_from(
            "test-device".into(),
            "te/device/main///e/click_event",
            event_json.to_string().as_str(),
        )?;

        assert_eq!(result.name, "click_event".to_string());
        let event_data = result.data.unwrap();
        assert_eq!(
            event_data.extras.get("extra").unwrap().as_str().unwrap(),
            "field"
        );
        assert_eq!(
            event_data.extras.get("numeric").unwrap().as_u64().unwrap(),
            32u64
        );
        assert_matches!(event_data.extras.get("complex"), Some(Value::Object(_)));

        Ok(())
    }

    #[test_case(
        "te/device/main///e/click_event",
        json!({
            "text": "Someone clicked",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone clicked".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: Some("main".into()),
        };
        "event parsing"
    )]
    #[test_case(
        "te/device/main///e/click_event",
        json!({
            "text": "Someone clicked",
        }),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone clicked".into()),
                time: None,
                extras: HashMap::new(),
            }),
            source: Some("main".into()),
        };
        "event parsing without timestamp"
    )]
    #[test_case(
        "te/device/main///e/click_event",
        json!({
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: Some("main".into()),
        };
        "event parsing without text"
    )]
    #[test_case(
        "te/device/main///e/click_event",
        json!({}),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
            source: Some("main".into()),
        };
        "event parsing without text or timestamp"
    )]
    #[test_case(
        "te/device/sensor///e/click_event",
        json!({
            "text": "Someone clicked",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone clicked".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: Some("main:device:sensor".into()),
        };
        "event parsing with sensor source"
    )]
    #[test_case(
        "te/device/main///e/click_event",
        json!({}),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
            source: Some("main".into()),
        };
        "event parsing empty payload with main source"
    )]
    #[test_case(
        "te/device/sensor/a/b/e/click_event",
        json!({
            "text": "Someone clicked",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone clicked".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: HashMap::new(),
            }),
            source: Some("main:device:sensor:a:b".into()),
        };
        "event parsing with sensor source more hierarchy"
    )]
    fn parse_new_thin_edge_event_json(
        event_topic: &str,
        event_payload: Value,
        expected_event: ThinEdgeEvent,
    ) {
        let event = ThinEdgeEvent::try_from(
            "main".into(),
            event_topic,
            event_payload.to_string().as_str(),
        )
        .unwrap();

        assert_eq!(event, expected_event);
    }
}
