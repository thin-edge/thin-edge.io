use crate::mqtt_topics::EntityTopicId;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::convert::TryFrom;
use tedge_utils::timestamp::deserialize_optional_string_or_unix_timestamp;
use time::OffsetDateTime;

/// Internal representation of ThinEdge alarm model.
#[derive(Debug, Eq, PartialEq)]
pub struct ThinEdgeAlarm {
    /// Alarm type retrieved from the MQTT topic or payload.
    /// The "type" given from the payload has priority to the one from the topic.
    pub alarm_type: String,

    /// Alarm source retrieved from the MQTT topic.
    pub source: EntityTopicId,

    /// All other alarm data retrieved from the MQTT payload.
    /// None means the message is meant to clear an alarm.
    pub data: Option<ThinEdgeAlarmData>,
}

/// Internal representation of the JSON MQTT payload.
#[derive(Debug, Deserialize, Eq, PartialEq)]
pub struct ThinEdgeAlarmData {
    pub severity: Option<String>,

    pub text: Option<String>,

    #[serde(default)]
    #[serde(deserialize_with = "deserialize_optional_string_or_unix_timestamp")]
    pub time: Option<OffsetDateTime>,

    #[serde(default)]
    #[serde(flatten)]
    pub extras: HashMap<String, Value>,
}

impl ThinEdgeAlarm {
    pub fn try_from(
        alarm_type: &str,
        source: &EntityTopicId,
        payload: &str,
    ) -> Result<Self, ThinEdgeAlarmDeserializerError> {
        let tedge_alarm = if payload.is_empty() {
            // Clearing an alarm
            ThinEdgeAlarm::new(alarm_type, source)
        } else {
            // Creating an alarm
            let tedge_alarm_data = ThinEdgeAlarmData::try_from(payload).map_err(|e| {
                ThinEdgeAlarmDeserializerError::FailedToParseJsonPayload {
                    alarm_type: alarm_type.to_string(),
                    error: e.to_string(),
                    payload: payload.chars().take(50).collect(),
                }
            })?;
            ThinEdgeAlarm::new(alarm_type, source).with_alarm_data(tedge_alarm_data)
        };

        Ok(tedge_alarm)
    }

    fn new(alarm_type: &str, source: &EntityTopicId) -> Self {
        Self {
            alarm_type: alarm_type.into(),
            source: source.clone(),
            data: None,
        }
    }

    /// Override "type" when it is provided in payload.
    fn with_alarm_data(self, mut data: ThinEdgeAlarmData) -> Self {
        match data.extras.remove("type") {
            Some(maybe_type_from_payload) => match maybe_type_from_payload.as_str() {
                Some(type_from_payload) => Self {
                    alarm_type: type_from_payload.to_string(),
                    data: Some(data),
                    ..self
                },
                None => Self {
                    data: Some(data),
                    ..self
                },
            },
            None => Self {
                data: Some(data),
                ..self
            },
        }
    }
}

/// Convert from JSON
impl TryFrom<&str> for ThinEdgeAlarmData {
    type Error = serde_json::error::Error;

    fn try_from(json: &str) -> Result<Self, Self::Error> {
        serde_json::from_str(json)
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeAlarmDeserializerError {
    #[error("Parsing of alarm message for the type: {alarm_type} failed due to error: {error}. Snipped payload: {payload}")]
    FailedToParseJsonPayload {
        alarm_type: String,
        error: String,
        payload: String,
    },

    #[error("Failed to parse as an UTF-8 string the payload received on topic: {topic}, due to error: {error}.")]
    FailedToParsePayloadToString { topic: String, error: String },
}

#[cfg(test)]
mod tests {
    use super::*;
    use maplit::hashmap;
    use serde_json::json;
    use serde_json::Value;
    use test_case::test_case;
    use time::macros::datetime;

    #[test_case(
        "temperature_alarm",
        EntityTopicId::default_main_device(),
        json!({
            "severity": "high",
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeAlarm {
            alarm_type: "temperature_alarm".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("high".into()),
                text: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00+05:00)),
                extras: hashmap!{},
            }),
        };
        "alarm parsing for main device"
    )]
    #[test_case(
        "temperature_alarm",
        EntityTopicId::default_main_device(),
        json!({
            "severity": "high",
            "text": "I raised it",
            "time": 1701954000,
        }),
        ThinEdgeAlarm {
            alarm_type: "temperature_alarm".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("high".into()),
                text: Some("I raised it".into()),
                time: Some(datetime!(2023-12-07 13:00:00 +00:00)),
                extras: hashmap!{},
            }),
        };
        "alarm parsing with unix timestamp"
    )]
    #[test_case(
        "temperature_alarm",
        EntityTopicId::default_child_device("extern_sensor").unwrap(),
        json!({
            "severity": "critical",
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeAlarm {
            alarm_type: "temperature_alarm".into(),
            source: EntityTopicId::default_child_device("extern_sensor").unwrap(),
            data: Some(ThinEdgeAlarmData {
                severity: Some("critical".into()),
                text: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                extras: hashmap!{},
            }),
        };
        "alarm parsing for child device"
    )]
    #[test_case(
        "temperature_alarm",
        EntityTopicId::default_main_device(),
        json!({
            "message": "Raised alarm with a message",
        }),
        ThinEdgeAlarm {
            alarm_type: "temperature_alarm".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: None,
                text: None,
                time: None,
                extras:hashmap!{"message".to_string() => json!("Raised alarm with a message".to_string())},
            }),
        };
        "alarm parsing with custom message"
    )]
    #[test_case(
        "temperature_alarm",
        EntityTopicId::default_main_device(),
        json!({
            "type": "new_alarm_type",
        }),
        ThinEdgeAlarm {
            alarm_type: "new_alarm_type".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: None,
                text: None,
                time: None,
                extras:hashmap!{},
            }),
        };
        "override alarm type"
    )]
    #[test_case(
        "",
        EntityTopicId::default_main_device(),
        json!({}),
        ThinEdgeAlarm {
            alarm_type: "".into(),
            source: EntityTopicId::default_main_device(),
            data: Some(ThinEdgeAlarmData {
                severity: None,
                text: None,
                time: None,
                extras:hashmap!{},
            }),
        };
        "alarm type accepts empty string"
    )]
    fn parse_thin_edge_alarm_json(
        alarm_type_from_topic: &str,
        entity: EntityTopicId,
        alarm_payload: Value,
        expected_alarm: ThinEdgeAlarm,
    ) {
        let json_string = alarm_payload.to_string();
        let alarm = ThinEdgeAlarm::try_from(alarm_type_from_topic, &entity, &json_string).unwrap();

        assert_eq!(alarm, expected_alarm);
    }
}
