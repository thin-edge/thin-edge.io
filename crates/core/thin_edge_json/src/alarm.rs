use std::convert::{TryFrom, TryInto};

use serde::Deserialize;
use time::OffsetDateTime;

/// In-memory representation of ThinEdge JSON alarm.
#[derive(Debug, Deserialize, PartialEq)]
pub struct ThinEdgeAlarm {
    pub name: String,
    pub severity: AlarmSeverity,
    pub data: Option<ThinEdgeAlarmData>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub enum AlarmSeverity {
    Critical,
    Major,
    Minor,
    Warning,
}

/// In-memory representation of ThinEdge JSON alarm payload
#[derive(Debug, Deserialize, PartialEq)]
pub struct ThinEdgeAlarmData {
    pub message: Option<String>,
    #[serde(default)]
    #[serde(deserialize_with = "clock::deserialize_iso8601_timestamp")]
    pub time: Option<OffsetDateTime>,
}

#[derive(thiserror::Error, Debug)]
pub enum ThinEdgeJsonDeserializerError {
    #[error("Unsupported topic: {0}")]
    UnsupportedTopic(String),

    #[error("Unsupported alarm severity in topic: {0}")]
    UnsupportedAlarmSeverity(String),

    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::error::Error),
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
            ))?,
        }
    }
}

impl ThinEdgeAlarm {
    pub fn try_from(
        mqtt_topic: &str,
        mqtt_payload: &str,
    ) -> Result<Self, ThinEdgeJsonDeserializerError> {
        let topic_split: Vec<&str> = mqtt_topic.split('/').collect();
        if topic_split.len() == 4 {
            let alarm_name = topic_split[3];
            if alarm_name.is_empty() {
                return Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(
                    mqtt_topic.into(),
                ));
            }

            let alarm_severity = topic_split[2];

            let alarm_data = if mqtt_payload.is_empty() {
                None
            } else {
                Some(serde_json::from_str(mqtt_payload)?)
            };

            Ok(Self {
                name: alarm_name.into(),
                severity: alarm_severity.try_into()?,
                data: alarm_data,
            })
        } else {
            return Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(
                mqtt_topic.into(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use serde_json::{json, Value};
    use test_case::test_case;
    use time::macros::datetime;

    #[test_case(
        "tedge/alarms/critical/temperature_alarm",
        json!({
            "message": "I raised it",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Critical,
            data: Some(ThinEdgeAlarmData {
                message: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        };
        "critical alarm parsing"
    )]
    #[test_case(
        "tedge/alarms/major/temperature_alarm",
        json!({
            "message": "I raised it",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Major,
            data: Some(ThinEdgeAlarmData {
                message: Some("I raised it".into()),
                time: None,
            }),
        };
        "major alarm parsing without timestamp"
    )]
    #[test_case(
        "tedge/alarms/minor/temperature_alarm",
        json!({
            "time": "2021-04-23T19:00:00+05:00",
        }),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Minor,
            data: Some(ThinEdgeAlarmData {
                message: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        };
        "minor alarm parsing without message"
    )]
    #[test_case(
        "tedge/alarms/warning/temperature_alarm",
        json!({}),
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Warning,
            data: Some(ThinEdgeAlarmData {
                message: None,
                time: None,
            }),
        };
        "warning alarm parsing without message or timestamp"
    )]
    fn parse_thin_edge_alarm_json(
        alarm_topic: &str,
        alarm_payload: Value,
        expected_alarm: ThinEdgeAlarm,
    ) {
        let alarm =
            ThinEdgeAlarm::try_from(alarm_topic, alarm_payload.to_string().as_str()).unwrap();

        assert_eq!(alarm, expected_alarm);
    }

    #[test]
    fn alarm_translation_empty_alarm_name() {
        let result = ThinEdgeAlarm::try_from("tedge/alarms/critical/", "{}");

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(_))
        );
    }

    #[test]
    fn alarm_translation_empty_severity() {
        let result = ThinEdgeAlarm::try_from("tedge/alarms//some_alarm", "{}");

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedAlarmSeverity(_))
        );
    }

    #[test]
    fn alarm_translation_empty_severity_and_name() {
        let result = ThinEdgeAlarm::try_from("tedge/alarms//", "{}");

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedTopic(_))
        );
    }

    #[test]
    fn alarm_translation_invalid_severity() {
        let result = ThinEdgeAlarm::try_from("tedge/alarms/invalid_severity/foo", "{}");

        assert_matches!(
            result,
            Err(ThinEdgeJsonDeserializerError::UnsupportedAlarmSeverity(_))
        );
    }

    #[test]
    fn alarm_translation_clear_alarm_with_empty_payload() {
        let result = ThinEdgeAlarm::try_from("tedge/alarms/critical/temperature_high_alarm", "");
        assert_matches!(result.unwrap().data, None);
    }
}
