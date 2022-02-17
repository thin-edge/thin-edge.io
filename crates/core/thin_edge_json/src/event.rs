use clock::Timestamp;
use serde::Deserialize;

use self::error::ThinEdgeJsonDeserializerError;

/// In-memory representation of ThinEdge JSON event.
#[derive(Debug, Deserialize, PartialEq)]
pub struct ThinEdgeEvent {
    pub name: String,
    pub data: Option<ThinEdgeEventData>,
}

/// In-memory representation of ThinEdge JSON event payload
#[derive(Debug, Deserialize, PartialEq)]
pub struct ThinEdgeEventData {
    pub message: Option<String>,

    #[serde(default)]
    #[serde(with = "clock::serde::rfc3339::option")]
    pub time: Option<Timestamp>,
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
    }
}

impl ThinEdgeEvent {
    pub fn try_from(
        mqtt_topic: &str,
        mqtt_payload: &str,
    ) -> Result<Self, ThinEdgeJsonDeserializerError> {
        let topic_split: Vec<&str> = mqtt_topic.split('/').collect();
        if topic_split.len() == 3 {
            let event_name = topic_split[2];
            if event_name.is_empty() {
                return Err(ThinEdgeJsonDeserializerError::EmptyEventName);
            }

            let event_data = if mqtt_payload.is_empty() {
                None
            } else {
                Some(serde_json::from_str(mqtt_payload)?)
            };

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
            "message": "Someone clicked",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: Some("Someone clicked".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        };
        "event parsing"
    )]
    #[test_case(
        "tedge/events/click_event",
        json!({
            "message": "Someone clicked",
        }),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: Some("Someone clicked".into()),
                time: None,
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
                message: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        };
        "event parsing without message"
    )]
    #[test_case(
        "tedge/events/click_event",
        json!({}),
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: None,
                time: None,
            }),
        };
        "event parsing without message or timestamp"
    )]
    fn parse_thin_edge_event_json(
        event_topic: &str,
        event_payload: Value,
        expected_event: ThinEdgeEvent,
    ) {
        let event =
            ThinEdgeEvent::try_from(event_topic, event_payload.to_string().as_str()).unwrap();

        assert_eq!(event, expected_event);
    }

    #[test]
    fn event_translation_empty_event_name() {
        let result = ThinEdgeEvent::try_from("tedge/events/", "{}");

        assert_matches!(result, Err(ThinEdgeJsonDeserializerError::EmptyEventName));
    }

    #[test]
    fn event_translation_more_than_three_topic_levels() {
        let result = ThinEdgeEvent::try_from("tedge/events/page/click", "{}");

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(_))
        );
    }

    #[test]
    fn event_translation_empty_payload() -> Result<()> {
        let result = ThinEdgeEvent::try_from("tedge/events/click_event", "")?;
        assert_eq!(result.name, "click_event".to_string());
        assert_matches!(result.data, None);

        Ok(())
    }
}
