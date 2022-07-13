use std::collections::HashMap;

use clock::Timestamp;
use json_writer::JsonWriter;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use self::error::ThinEdgeJsonDeserializerError;

/// In-memory representation of ThinEdge JSON event.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ThinEdgeEvent {
    #[serde(rename = "type")]
    pub name: String,
    #[serde(flatten)]
    pub data: Option<ThinEdgeEventData>,
}

/// In-memory representation of ThinEdge JSON event payload
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ThinEdgeEventData {
    pub text: Option<String>,

    #[serde(default)]
    #[serde(with = "clock::serde::rfc3339::option")]
    pub time: Option<Timestamp>,

    #[serde(flatten)]
    pub extras: HashMap<String, Value>,
}

pub mod error {
    use json_writer::JsonWriterError;

    #[derive(thiserror::Error, Debug)]
    pub enum ThinEdgeJsonDeserializerError {
        #[error("Unsupported topic: {0}")]
        UnsupportedTopic(String),

        #[error("Event name can not be empty")]
        EmptyEventName,

        #[error(transparent)]
        SerdeJsonError(#[from] serde_json::error::Error),

        #[error(transparent)]
        JsonWriterError(#[from] JsonWriterError),
    }
}

impl ThinEdgeEvent {
    pub fn try_from(
        mqtt_topic: &str,
        mqtt_payload: &str,
        may_be_child: Option<&str>,
    ) -> Result<Self, ThinEdgeJsonDeserializerError> {
        let topic_split: Vec<&str> = mqtt_topic.split('/').collect();
        if topic_split.len() >= 3 {
            let event_name = topic_split[2];
            if event_name.is_empty() {
                return Err(ThinEdgeJsonDeserializerError::EmptyEventName);
            }

            let mut event_data = if mqtt_payload.is_empty() {
                None
            } else {
                Some(serde_json::from_str(mqtt_payload)?)
            };

            if let Some(c_id) = may_be_child {
                let mut json = JsonWriter::with_capacity(1024);
                json.write_open_obj();
                let _ = json.write_key("externalId");
                let _ = json.write_str(c_id);
                let _ = json.write_key("type");
                let _ = json.write_str("c8y_Serial");
                let _ = json.write_close_obj();
                event_data.as_mut().map(|e: &mut ThinEdgeEventData| {
                    e.extras.insert(
                        "externalSource".into(),
                        serde_json::from_str(&json.into_string().ok()?).ok()?,
                    )
                });
            }

            Ok(Self {
                name: event_name.into(),
                data: event_data,
            })
        } else {
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(
                mqtt_topic.into(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use assert_matches::assert_matches;
    use serde_json::{json, Value};
    use test_case::test_case;
    use time::macros::datetime;

    #[test_case(
        "tedge/events/click_event",
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
        };
        "event parsing"
    )]
    #[test_case(
        "tedge/events/click_event",
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
        };
        "event parsing without timestamp"
    )]
    #[test_case(
        "tedge/events/click_event",
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
        };
        "event parsing without text"
    )]
    #[test_case(
        "tedge/events/click_event",
        json!({}),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
        };
        "event parsing without text or timestamp"
    )]
    fn parse_thin_edge_event_json(
        event_topic: &str,
        event_payload: Value,
        expected_event: ThinEdgeEvent,
    ) {
        let event =
            ThinEdgeEvent::try_from(event_topic, event_payload.to_string().as_str(), None).unwrap();

        assert_eq!(event, expected_event);
    }

    #[test]
    fn event_translation_empty_event_name() {
        let result = ThinEdgeEvent::try_from("tedge/events/", "{}", None);

        assert_matches!(result, Err(ThinEdgeJsonDeserializerError::EmptyEventName));
    }

    #[test]
    fn event_translation_more_than_three_topic_levels() {
        let result = ThinEdgeEvent::try_from("tedge/events/page/click", "{}", None);

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(_))
        );
    }

    #[test]
    fn event_translation_for_child_dev() {
        let pay_load = json!({
            "text": "Someone clicked",
            "time": "2021-04-23T19:00:00+05:00",
        });
        let mut extras = HashMap::new();
        let mut json = JsonWriter::with_capacity(1024);
        json.write_open_obj();
        let _ = json.write_key("externalId");
        let _ = json.write_str("test_child");
        let _ = json.write_key("type");
        let _ = json.write_str("c8y_Serial");
        let _ = json.write_close_obj();
        extras.insert(
            "externalSource".into(),
            serde_json::from_str(&json.into_string().unwrap()).unwrap(),
        );

        let expected_event = ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: Some("Someone clicked".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: extras,
            }),
        };
        let event = ThinEdgeEvent::try_from(
            "tedge/events/click_event/test_child",
            pay_load.to_string().as_str(),
            Some("test_child"),
        )
        .unwrap();
        assert_eq!(event, expected_event);
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
        let result = ThinEdgeEvent::try_from("tedge/events/click_event", "", None)?;
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
            "tedge/events/click_event",
            event_json.to_string().as_str(),
            None,
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
}
