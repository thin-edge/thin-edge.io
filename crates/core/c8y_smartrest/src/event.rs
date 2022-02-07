use thin_edge_json::event::ThinEdgeEvent;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::error::SmartRestSerializerError;

/// Converts from thin-edge event to C8Y event SmartREST message
pub fn serialize_event(event: ThinEdgeEvent) -> Result<String, SmartRestSerializerError> {
    match event.data {
        None => Ok(format!("400,{},", event.name)),
        Some(event_data) => {
            let current_timestamp = OffsetDateTime::now_utc();

            let smartrest_message = format!(
                "400,{},\"{}\",{}",
                event.name,
                event_data.message.unwrap_or_default(),
                event_data.time.map_or_else(
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
    use thin_edge_json::event::ThinEdgeEventData;
    use time::macros::datetime;

    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: Some("I raised it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "400,click_event,\"I raised it\",2021-04-23T19:00:00+05:00"
        ;"event translation"
    )]
    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: None,
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "400,click_event,\"\",2021-04-23T19:00:00+05:00"
        ;"event translation without message"
    )]
    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: Some("I, raised, it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "400,click_event,\"I, raised, it\",2021-04-23T19:00:00+05:00"
        ;"event translation with commas in message"
    )]
    fn check_event_translation(event: ThinEdgeEvent, expected_smartrest_msg: &str) {
        let result = serialize_event(event);

        assert_eq!(result.unwrap(), expected_smartrest_msg);
    }

    #[derive(Debug, Deserialize)]
    struct SmartRestEvent {
        pub code: i32,
        pub name: String,
        pub message: Option<String>,
        pub time: Option<OffsetDateTime>,
    }

    #[test]
    fn event_translation_empty_json_payload_generates_timestamp() {
        let event = ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: Some("I raised it".into()),
                time: None,
            }),
        };

        let smartrest_message = serialize_event(event).unwrap();
        let mut reader = csv::Reader::from_reader(smartrest_message.as_bytes());
        for result in reader.deserialize() {
            let smartrest_event: SmartRestEvent = result.unwrap();
            assert_eq!(smartrest_event.code, 301);
            assert_eq!(smartrest_event.name, "empty_event".to_string());
            assert_eq!(smartrest_event.message, None);
            assert_matches!(smartrest_event.time, Some(_))
        }
    }
}
