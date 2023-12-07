use crate::json_c8y::AlarmSeverity;
use crate::json_c8y::C8yAlarm;
use crate::smartrest::csv::fields_to_csv_string;
use time::format_description::well_known::Rfc3339;

/// Serialize C8yAlarm to SmartREST message
pub fn serialize_alarm(c8y_alarm: &C8yAlarm) -> Result<String, time::error::Format> {
    let smartrest = match c8y_alarm {
        C8yAlarm::Create(alarm) => {
            let smartrest_code = match alarm.severity {
                AlarmSeverity::Critical => "301",
                AlarmSeverity::Major => "302",
                AlarmSeverity::Minor => "303",
                AlarmSeverity::Warning => "304",
            };
            fields_to_csv_string(&[
                smartrest_code,
                &alarm.alarm_type,
                &alarm.text,
                &alarm.time.format(&Rfc3339)?,
            ])
        }
        C8yAlarm::Clear(alarm) => fields_to_csv_string(&["306", &alarm.alarm_type]),
    };
    Ok(smartrest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_c8y::C8yClearAlarm;
    use crate::json_c8y::C8yCreateAlarm;
    use crate::json_c8y::SourceInfo;
    use maplit::hashmap;
    use test_case::test_case;
    use time::macros::datetime;

    #[test_case(
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature_alarm".into(),
            source: None,
            severity: AlarmSeverity::Critical,
            text: "I raised it".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: hashmap!{},
        }),
        "301,temperature_alarm,I raised it,2021-04-23T19:00:00+05:00"
        ;"critical alarm translation"
    )]
    #[test_case(
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature_alarm".into(),
            source: None,
            severity: AlarmSeverity::Major,
            text: "I raised it".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: hashmap!{},
        }),
        "302,temperature_alarm,I raised it,2021-04-23T19:00:00+05:00"
        ;"major alarm translation"
    )]
    #[test_case(
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature_alarm".into(),
            source: None,
            severity: AlarmSeverity::Minor,
            text: "".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: hashmap!{},
        }),
        "303,temperature_alarm,,2021-04-23T19:00:00+05:00"
        ;"minor alarm translation without message"
    )]
    #[test_case(
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature_alarm".into(),
            source: None,
            severity: AlarmSeverity::Warning,
            text: "I, raised, it".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: hashmap!{},
        }),
        "304,temperature_alarm,\"I, raised, it\",2021-04-23T19:00:00+05:00"
        ;"warning alarm translation with commas in message"
    )]
    #[test_case(
        C8yAlarm::Create(C8yCreateAlarm {
            alarm_type: "temperature_alarm".into(),
            source: Some(SourceInfo {
                id: "External_source".into(),
                source_type: "c8y_Serial".into()
            }),
            severity: AlarmSeverity::Warning,
            text: "External sensor raised alarm".into(),
            time: datetime!(2021-04-23 19:00:00 +05:00),
            fragments: hashmap!{},
        }),
        "304,temperature_alarm,External sensor raised alarm,2021-04-23T19:00:00+05:00"
        ;"warning alarm translation by external sensor"
    )]
    #[test_case(
        C8yAlarm::Clear(C8yClearAlarm {
            alarm_type: "temperature_alarm".into(),
            source: None,
        }),
        "306,temperature_alarm"
        ;"clear alarm translation"
    )]
    #[test_case(
        C8yAlarm::Clear(C8yClearAlarm {
            alarm_type: "temperature_alarm".into(),
            source: Some(SourceInfo {
                id: "External_source".into(),
                source_type: "c8y_Serial".into()
            }),
        }),
        "306,temperature_alarm"
        ;"clear child alarm translation"
    )]
    fn check_alarm_translation(alarm: C8yAlarm, expected_smartrest_msg: &str) {
        let smartrest = serialize_alarm(&alarm);
        assert_eq!(smartrest.unwrap(), expected_smartrest_msg);
    }
}
