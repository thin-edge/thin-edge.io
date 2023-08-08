use clock::Timestamp;
use serde::Deserialize;
use serde::Serialize;
use std::convert::TryFrom;
use std::fmt;

use serde_json::Value;
use std::collections::HashMap;

use crate::device_id::get_external_identity_from_topic;

/// In-memory representation of ThinEdge JSON alarm.
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq)]
pub struct ThinEdgeAlarm {
    pub name: String,
    #[serde(flatten)]
    pub data: Option<ThinEdgeAlarmData>,
    pub source: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub enum AlarmSeverity {
    #[serde(alias = "critical")]
    Critical,
    #[serde(alias = "major")]
    Major,
    #[serde(alias = "minor")]
    Minor,
    #[serde(alias = "warning")]
    Warning,
}

/// In-memory representation of ThinEdge JSON alarm payload
#[derive(Debug, Serialize, Deserialize, Eq, PartialEq, Clone)]
pub struct ThinEdgeAlarmData {
    pub text: Option<String>,
    #[serde(default = "default_severity")]
    pub severity: AlarmSeverity,
    #[serde(default)]
    #[serde(with = "time::serde::rfc3339::option")]
    pub time: Option<Timestamp>,

    #[serde(flatten)]
    pub alarm_data: HashMap<String, Value>,
}

fn default_severity() -> AlarmSeverity {
    AlarmSeverity::Major
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonDeserializerError {
    #[error("Unsupported topic: {0}")]
    UnsupportedTopic(String),

    #[error("Unsupported alarm severity in topic: {0}")]
    UnsupportedAlarmSeverity(String),

    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::error::Error),

    #[error("Unsupported external device ID in topic: {0}")]
    UnsupportedExternalDeviceId(String),

    #[error("Parsing of alarm message received on topic: {topic} failed due to error: {error}. Snipped payload: {payload}")]
    FailedToParseJsonPayload {
        topic: String,
        error: String,
        payload: String,
    },

    #[error("Failed to parse as an UTF-8 string the payload received on topic: {topic}, due to error: {error}.")]
    FailedToParsePayloadToString { topic: String, error: String },
}

impl TryFrom<&str> for AlarmSeverity {
    type Error = ThinEdgeJsonDeserializerError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "critical" => Ok(AlarmSeverity::Critical),
            "major" => Ok(AlarmSeverity::Major),
            "minor" => Ok(AlarmSeverity::Minor),
            "warning" => Ok(AlarmSeverity::Warning),
            invalid => Err(ThinEdgeJsonDeserializerError::UnsupportedAlarmSeverity(
                invalid.into(),
            )),
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

impl ThinEdgeAlarm {
    /// parent_device_name is needed to create the child device external id
    pub fn try_from(
        parent_device_name: String,
        mqtt_topic: &str,
        mqtt_payload: &str,
    ) -> Result<Self, ThinEdgeJsonDeserializerError> {
        validate_alarm_topic(mqtt_topic)?;
        let topic_split: Vec<&str> = mqtt_topic.split('/').collect();

        let alarm_name = topic_split.last().cloned().unwrap_or_default();
        if alarm_name.is_empty() {
            return Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(
                mqtt_topic.into(),
            ));
        }
        let alarm_data: Option<ThinEdgeAlarmData> = if mqtt_payload.is_empty() {
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
            name: alarm_name.into(),
            data: alarm_data,
            source: Some(external_source),
        })
    }
}

fn validate_alarm_topic(topic: &str) -> Result<(), ThinEdgeJsonDeserializerError> {
    match topic.split('/').collect::<Vec<_>>()[..] {
        ["te", "device", device_id, _, _, "a", _alarm_type] => {
            dbg!(&device_id);
            if device_id.is_empty() {
                Err(ThinEdgeJsonDeserializerError::UnsupportedExternalDeviceId(
                    device_id.into(),
                ))
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
    use assert_matches::assert_matches;
    use maplit::hashmap;
    use serde_json::json;
    use serde_json::Value;
    use test_case::test_case;
    use time::macros::datetime;

    #[test_case(
        "te/device/main///a/temperature_alarm",
        json!({
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00",
            "severity": "critical",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),           
            data: Some(ThinEdgeAlarmData {
                text: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                alarm_data: hashmap!{},
                severity: AlarmSeverity::Critical,
            }),
            source: Some("test-device".into()),
        };
        "critical alarm parsing"
    )]
    #[test_case(
        "te/device/main///a/temperature_alarm",
        json!({
            "text": "I raised it",
            "severity":"major",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),          
            data: Some(ThinEdgeAlarmData {
                text: Some("I raised it".into()),
                time: None,
                alarm_data: hashmap!{},
                severity: AlarmSeverity::Major,
            }),
            source: Some("test-device".into()),
        };
        "major alarm parsing without timestamp"
    )]
    #[test_case(
        "te/device/main///a/temperature_alarm",
        json!({
            "time": "2021-04-23T19:00:00+05:00",
            "severity": "minor",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            data: Some(ThinEdgeAlarmData {
                text: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                alarm_data: hashmap!{},
                severity: AlarmSeverity::Minor,
            }),
            source: Some("test-device".into()),
        };
        "minor alarm parsing without text"
    )]
    #[test_case(
        "te/device/main///a/temperature_alarm",
        json!({}),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),           
            data: Some(ThinEdgeAlarmData {
                text: None,
                time: None,
                alarm_data: hashmap!{},
                severity: AlarmSeverity::Major,
            }),
            source: Some("test-device".into()),
        };
        "alarm parsing with empty json payload default major severity"
    )]
    #[test_case(
        "te/device/external_sensor///a/temperature_alarm",
        json!({
            "text": "I raised it",
            "time": "2021-04-23T19:00:00+05:00",
            "severity": "major",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),           
            data: Some(ThinEdgeAlarmData {
                text: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                alarm_data: hashmap!{},
                severity: AlarmSeverity::Major,
            }),
            source: Some("test-device:device:external_sensor".to_string()),
        };
        "critical alarm parsing with childId"
    )]
    #[test_case(
        "te/device/external_sensor///a/temperature_alarm",
        json!({
            "text": "I raised it",
            "message": "Raised alarm with a message",
            "time": "2021-04-23T19:00:00+05:00",
            "severity": "critical",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),          
            data: Some(ThinEdgeAlarmData {
                text: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                alarm_data:hashmap!{"message".to_string() => json!("Raised alarm with a message".to_string())},
                severity: AlarmSeverity::Critical,
            }),
            source: Some("test-device:device:external_sensor".to_string()),
        };
        "critical alarm parsing with text and custom message with childid"
    )]
    #[test_case(
        "te/device/external_sensor///a/temperature_alarm",
        json!({
            "message": "Raised alarm with a message",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),          
            data: Some(ThinEdgeAlarmData {
                text: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
                alarm_data: hashmap!{"message".to_string() => json!("Raised alarm with a message".to_string())},
                severity: AlarmSeverity::Major,
            }),
            source: Some("test-device:device:external_sensor".to_string()),
        };
        "critical alarm parsing for child no text and with custom message"
    )]
    fn parse_thin_edge_alarm_json(
        alarm_topic: &str,
        alarm_payload: Value,
        expected_alarm: ThinEdgeAlarm,
    ) {
        let alarm = ThinEdgeAlarm::try_from(
            "test-device".into(),
            alarm_topic,
            alarm_payload.to_string().as_str(),
        )
        .unwrap();

        assert_eq!(alarm, expected_alarm);
    }

    #[test]
    fn alarm_translation_empty_alarm_name() {
        let result =
            ThinEdgeAlarm::try_from("test-device".into(), "te/device/external_sensor///a/", "{}");

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(_))
        );
    }

    #[test]
    fn alarm_translation_empty_severity() {
        let result = ThinEdgeAlarm::try_from(
            "test-device".into(),
            "te/device/main///a/some_alarm",
            r#"{"severity":"test_severity"}"#,
        );

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::SerdeJsonError(_))
        );
        assert_eq!(
                result.unwrap_err().to_string(),
                "unknown variant `test_severity`, expected one of `Critical`, `Major`, `Minor`, `Warning` at line 1 column 27");
    }

    #[test]
    fn alarm_translation_empty_severity_and_name() {
        let result = ThinEdgeAlarm::try_from(
            "test-device".into(),
            "te/device/main///a/some_alarm",
            r#"{"severity":""}"#,
        );

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::SerdeJsonError(_))
        );
        assert_eq!(
                result.unwrap_err().to_string(),
                "unknown variant ``, expected one of `Critical`, `Major`, `Minor`, `Warning` at line 1 column 14");
    }

    #[test]
    fn alarm_translation_no_severity() {
        let result = ThinEdgeAlarm::try_from(
            "test-device".into(),
            "te/device/main///a/some_alarm",
            r#"{"text":"no severity alarm"}"#,
        );

        assert_eq!(result.unwrap().data.unwrap().severity, AlarmSeverity::Major);
    }

    #[test]
    fn alarm_translation_clear_alarm_with_empty_payload() {
        let result = ThinEdgeAlarm::try_from(
            "test-device".into(),
            "te/device/main///a/temperature_high_alarm",
            "",
        );
        assert_matches!(result.unwrap().data, None);
    }

    #[test]
    fn alarm_translation_invalid_topic_levels() {
        let result = ThinEdgeAlarm::try_from(
            "test-device".into(),
            "te/device/main///a/temperature_alarm//",
            "{}",
        );
        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(_))
        );
    }

    #[test]
    fn child_alarm_translation_empty_external_device_name() {
        let result = ThinEdgeAlarm::try_from(
            "test-device".into(),
            "te/device////a/temperature_alarm",
            "{}",
        );

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedExternalDeviceId(
                _
            ))
        );
    }

    #[test]
    fn child_alarm_translation_empty_alarm_name() {
        let result =
            ThinEdgeAlarm::try_from("test-device".into(), "te/device/external_sensor///a/", "{}");

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(_))
        );
    }
}
