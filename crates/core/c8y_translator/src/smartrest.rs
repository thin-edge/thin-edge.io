use chrono::prelude::*;
use clock::{Clock, WallClock};
use serde::{Deserialize, Serialize};
use serde_json;

use crate::json::CumulocityJsonError;

#[derive(Debug, Serialize, Deserialize)]
pub struct ThinEdgeJsonAlarm {
    pub message: Option<String>,
    pub time: Option<DateTime<FixedOffset>>,
}

/// Converts from thin-edge alarm JSON to C8Y alarm JSON
pub fn from_thin_edge_alarm_json(
    name: &str,
    severity: &str,
    payload: &str,
) -> Result<String, CumulocityJsonError> {
    if payload.is_empty() {
        Ok(format!("306,{}", name))
    } else {
        let current_timestamp = WallClock.now();
        let tedge_alarm_json: ThinEdgeJsonAlarm = serde_json::from_str(payload)?;

        let smartrest_code = match severity {
            "critical" => 301,
            "major" => 302,
            "minor" => 303,
            "warning" => 304,
            invalid => Err(CumulocityJsonError::UnsupportedAlarmSeverity(
                invalid.into(),
            ))?,
        };

        let smartrest_message = format!(
            "{},{},{},{}",
            smartrest_code,
            name,
            tedge_alarm_json.message.unwrap_or_default(),
            tedge_alarm_json.time.map_or_else(
                || current_timestamp.to_rfc3339(),
                |timestamp| timestamp.to_rfc3339()
            )
        );

        Ok(smartrest_message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use serde_json::{json, Value};
    use test_case::test_case;

    #[test_case(
        "temperature_alarm",
        "critical",
        json!({
            "message": "I raised it",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        "301,temperature_alarm,I raised it,2021-04-23T19:00:00+05:00"
        ;"critical alarm translation"
    )]
    #[test_case(
        "temperature_alarm",
        "major",
        json!({
            "message": "You raised it",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        "302,temperature_alarm,You raised it,2021-04-23T19:00:00+05:00"
        ;"major alarm translation without timestamp"
    )]
    #[test_case(
        "temperature_alarm",
        "minor",
        json!({
            "time": "2021-04-23T19:00:00+05:00",
        }),
        "303,temperature_alarm,,2021-04-23T19:00:00+05:00"
        ;"minor alarm translation without message"
    )]
    #[test_case(
        "temperature_alarm",
        "warning",
        json!({
            "message": "We raised it",
            "time": "2021-04-23T19:00:00+05:00",
        }),
        "304,temperature_alarm,We raised it,2021-04-23T19:00:00+05:00"
        ;"warning alarm translation"
    )]
    fn check_alarm_translation(
        alarm_name: &str,
        alarm_severity: &str,
        alarm_payload: Value,
        expected_smartrest_msg: &str,
    ) {
        let result = from_thin_edge_alarm_json(
            alarm_name,
            alarm_severity,
            alarm_payload.to_string().as_str(),
        );

        assert_eq!(result.unwrap(), expected_smartrest_msg);
    }

    #[test]
    fn alarm_translation_invalid_severity() {
        let result = from_thin_edge_alarm_json("invalid_alarm", "foo", "{}");

        assert_matches!(
            result,
            Err(CumulocityJsonError::UnsupportedAlarmSeverity(_))
        );
    }

    #[derive(Debug, Deserialize)]
    struct SmartRestAlarm {
        pub code: i32,
        pub name: String,
        pub message: Option<String>,
        pub time: Option<DateTime<FixedOffset>>,
    }

    #[test]
    fn alarm_translation_empty_json_payload_generates_timestamp() {
        let smartrest_message = from_thin_edge_alarm_json("empty_alarm", "critical", "{}").unwrap();
        let mut reader = csv::Reader::from_reader(smartrest_message.as_bytes());
        for result in reader.deserialize() {
            let smartrest_alarm: SmartRestAlarm = result.unwrap();
            assert_eq!(smartrest_alarm.code, 301);
            assert_eq!(smartrest_alarm.name, "empty_alarm".to_string());
            assert_eq!(smartrest_alarm.message, None);
            assert_matches!(smartrest_alarm.time, Some(_))
        }
    }

    #[test]
    fn alarm_translation_clear_alarm_with_empty_payload() {
        let result = from_thin_edge_alarm_json("some_alarm", "critical", "");
        assert_eq!(result.unwrap(), "306,some_alarm")
    }
}
