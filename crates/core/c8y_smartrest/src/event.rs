use thin_edge_json::event::ThinEdgeEvent;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::error::SmartRestSerializerError;

const CREATE_EVENT_SMARTREST_CODE: u16 = 400;

/// Converts from thin-edge event to C8Y event SmartREST message
pub fn serialize_event(event: ThinEdgeEvent) -> Result<String, SmartRestSerializerError> {
    let current_timestamp = OffsetDateTime::now_utc();
    match event.data {
        None => Ok(format!(
            "{CREATE_EVENT_SMARTREST_CODE},{},{},{}",
            event.name,
            event.name,
            current_timestamp.format(&Rfc3339)?
        )),
        Some(event_data) => {
            let smartrest_message = format!(
                "{CREATE_EVENT_SMARTREST_CODE},{},\"{}\",{}",
                event.name.clone(),
                event_data.message.unwrap_or(event.name),
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
    use anyhow::Result;
    use assert_matches::assert_matches;
    use serde::Deserialize;
    use test_case::test_case;
    use thin_edge_json::event::ThinEdgeEventData;
    use time::macros::datetime;

    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: Some("Someone clicked".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "400,click_event,\"Someone clicked\",2021-04-23T19:00:00+05:00"
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
        "400,click_event,\"click_event\",2021-04-23T19:00:00+05:00"
        ;"event translation without message"
    )]
    #[test_case(
        ThinEdgeEvent {
            name: "click_event".into(),
            data: Some(ThinEdgeEventData {
                message: Some("Someone, clicked, it".into()),
                time: Some(datetime!(2021-04-23 19:00:00 +05:00)),
            }),
        },
        "400,click_event,\"Someone, clicked, it\",2021-04-23T19:00:00+05:00"
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
        pub time: Option<String>,
    }

    #[test]
    fn event_translation_empty_json_payload_generates_timestamp() -> Result<()> {
        let event = ThinEdgeEvent {
            name: "empty_event".into(),
            data: Some(ThinEdgeEventData {
                message: None,
                time: None,
            }),
        };

        let smartrest_message = serialize_event(event)?;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(smartrest_message.as_bytes());
        let mut iter = reader.deserialize();
        let result = iter.next();

        assert!(result.is_some());
        let smartrest_event: SmartRestEvent = result.expect("One entry expected")?;
        assert_eq!(smartrest_event.code, 400);
        assert_eq!(smartrest_event.name, "empty_event".to_string());
        assert_eq!(smartrest_event.message, Some("empty_event".to_string()));
        assert_matches!(smartrest_event.time, Some(_));

        Ok(())
    }

    #[test]
    fn event_translation_empty_payload() -> Result<()> {
        let event = ThinEdgeEvent {
            name: "empty_event".into(),
            data: None,
        };

        let smartrest_message = serialize_event(event)?;
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .from_reader(smartrest_message.as_bytes());
        let mut iter = reader.deserialize();
        let result = iter.next();

        assert!(result.is_some());
        let smartrest_event: SmartRestEvent = result.expect("One entry expected")?;
        assert_eq!(smartrest_event.code, 400);
        assert_eq!(smartrest_event.name, "empty_event".to_string());
        assert_eq!(smartrest_event.message, Some("empty_event".to_string()));
        assert_matches!(smartrest_event.time, Some(_));

        Ok(())
    }
}
