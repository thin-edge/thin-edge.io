use thin_edge_json::alarm::{AlarmSeverity, ThinEdgeAlarm};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::error::SmartRestSerializerError;

/// Converts from thin-edge alarm to C8Y alarm SmartREST message
pub fn serialize_alarm(alarm: ThinEdgeAlarm) -> Result<String, SmartRestSerializerError> {
    match alarm.data {
        None => Ok(format!("306,{}", alarm.name)),
        Some(alarm_data) => {
            let smartrest_code = match alarm.severity {
                AlarmSeverity::Critical => 301,
                AlarmSeverity::Major => 302,
                AlarmSeverity::Minor => 303,
                AlarmSeverity::Warning => 304,
            };

            let current_timestamp = OffsetDateTime::now_utc();

            let smartrest_message = format!(
                "{},{},\"{}\",{}",
                smartrest_code,
                alarm.name,
                alarm_data.message.unwrap_or_default(),
                alarm_data.time.map_or_else(
                    || current_timestamp.format(&Rfc3339),
                    |timestamp| timestamp.format(&Rfc3339)
                )?
            );

            Ok(smartrest_message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use serde::Deserialize;
    use test_case::test_case;
    use thin_edge_json::alarm::ThinEdgeAlarmData;
    use time::macros::datetime;

    #[test_case(
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Critical,
            data: Some(ThinEdgeAlarmData {
                message: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "301,temperature_alarm,\"I raised it\",2021-04-23T19:00:00+05:00"
        ;"critical alarm translation"
    )]
    #[test_case(
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Major,
            data: Some(ThinEdgeAlarmData {
                message: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "302,temperature_alarm,\"I raised it\",2021-04-23T19:00:00+05:00"
        ;"major alarm translation"
    )]
    #[test_case(
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Minor,
            data: Some(ThinEdgeAlarmData {
                message: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "303,temperature_alarm,\"\",2021-04-23T19:00:00+05:00"
        ;"minor alarm translation without message"
    )]
    #[test_case(
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Warning,
            data: Some(ThinEdgeAlarmData {
                message: Some("I, raised, it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "304,temperature_alarm,\"I, raised, it\",2021-04-23T19:00:00+05:00"
        ;"warning alarm translation with commas in message"
    )]
    #[test_case(
        ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Minor,
            data: None,
        },
        "306,temperature_alarm"
        ;"clear alarm translation"
    )]
    fn check_alarm_translation(alarm: ThinEdgeAlarm, expected_smartrest_msg: &str) {
        let result = serialize_alarm(alarm);

        assert_eq!(result.unwrap(), expected_smartrest_msg);
    }

    #[derive(Debug, Deserialize)]
    struct SmartRestAlarm {
        pub code: i32,
        pub name: String,
        pub message: Option<String>,
        pub time: Option<OffsetDateTime>,
    }

    #[test]
    fn alarm_translation_empty_json_payload_generates_timestamp() {
        let alarm = ThinEdgeAlarm {
            name: "temperature_alarm".into(),
            severity: AlarmSeverity::Warning,
            data: Some(ThinEdgeAlarmData {
                message: Some("I raised it".into()),
                time: None,
            }),
        };

        let smartrest_message = serialize_alarm(alarm).unwrap();
        let mut reader = csv::Reader::from_reader(smartrest_message.as_bytes());
        for result in reader.deserialize() {
            let smartrest_alarm: SmartRestAlarm = result.unwrap();
            assert_eq!(smartrest_alarm.code, 301);
            assert_eq!(smartrest_alarm.name, "empty_alarm".to_string());
            assert_eq!(smartrest_alarm.message, None);
            assert_matches!(smartrest_alarm.time, Some(_))
        }
    }
}
