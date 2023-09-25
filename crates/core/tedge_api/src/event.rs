use self::error::ThinEdgeJsonDeserializerError;
use crate::entity_store::EntityMetadata;
use clock::Timestamp;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

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
        #[error(transparent)]
        SerdeJsonError(#[from] serde_json::error::Error),

        #[error("Parsing of event message received on topic: {topic} failed due to error: {error}. Snipped payload: {payload}")]
        FailedToParseJsonPayload {
            topic: String,
            error: String,
            payload: String,
        },

        #[error("Failed to parse as an UTF-8 string the payload received on topic: {topic}, due to error: {error}.")]
        FailedToParsePayloadToString { topic: String, error: String },
    }
}

impl ThinEdgeEvent {
    pub fn try_from(
        event_type: &str,
        entity: &EntityMetadata,
        mqtt_payload: &str,
    ) -> Result<Self, ThinEdgeJsonDeserializerError> {
        let event_data = if mqtt_payload.is_empty() {
            None
        } else {
            Some(serde_json::from_str(mqtt_payload)?)
        };

        // Parent exists means the device is child device
        let external_source = entity.parent.as_ref().map(|_| entity.external_id.clone());

        Ok(Self {
            name: event_type.into(),
            data: event_data,
            source: external_source.map(|v| v.into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use serde_json::json;
    use serde_json::Value;
    use test_case::test_case;
    use time::macros::datetime;

    #[test_case(
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
            source: None,
        };
        "event parsing"
    )]
    #[test_case(
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
            source: None,
        };
        "event parsing without timestamp"
    )]
    #[test_case(
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
            source: None,
        };
        "event parsing without text"
    )]
    #[test_case(
        json!({}),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
            source: None,
        };
        "event parsing without text or timestamp"
    )]
    fn parse_thin_edge_event_json(event_payload: Value, expected_event: ThinEdgeEvent) {
        let event_type = "click_event";
        let entity = EntityMetadata::main_device("main-device".to_string());
        let event =
            ThinEdgeEvent::try_from(event_type, &entity, event_payload.to_string().as_str())
                .unwrap();

        assert_eq!(event, expected_event);
    }

    #[test_case(
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
            source: Some("external_source".into()),
        };
        "event parsing with external source"
    )]
    #[test_case(
        json!({}),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                text: None,
                time: None,
                extras: HashMap::new(),
            }),
            source: Some("external_source".into()),
        };
        "event parsing empty payload with external source"
    )]
    fn parse_thin_edge_event_json_from_extra_source(
        event_payload: Value,
        expected_event: ThinEdgeEvent,
    ) {
        let event_type = "click_event";
        let entity = EntityMetadata::child_device("external_source".to_string()).unwrap();
        let event =
            ThinEdgeEvent::try_from(event_type, &entity, event_payload.to_string().as_str())
                .unwrap();

        assert_eq!(event, expected_event);
    }

    #[test]
    fn event_translation_empty_payload() {
        let event_data = ThinEdgeEventData {
            text: Some("foo".to_string()),
            time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            extras: HashMap::new(),
        };

        let serialized = serde_json::to_string(&event_data).unwrap();
        assert_eq!(
            r#"{"text":"foo","time":"2021-04-23T19:00:00+05:00"}"#,
            serialized
        );
    }

    #[test]
    fn test_serialize() {
        let entity = EntityMetadata::main_device("main-device".to_string());
        let result = ThinEdgeEvent::try_from("click_event", &entity, "").unwrap();

        assert_eq!(result.name, "click_event".to_string());
        assert_matches!(result.data, None);
    }

    #[test]
    fn event_translation_additional_fields() {
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

        let entity = EntityMetadata::main_device("main-device".to_string());

        let result =
            ThinEdgeEvent::try_from("click_event", &entity, event_json.to_string().as_str())
                .unwrap();
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
    }
}
